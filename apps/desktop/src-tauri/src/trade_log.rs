//! Read-only parser for confirmed Warframe trades written to `EE.log`.
//!
//! The dialog shape and state-machine safeguards follow the approach used by
//! `TennoWorth` (MIT): a dialog is buffered, sealed by the next framework line and
//! emitted only after the game's explicit success marker.

use platscope_storage::TradeItem;

pub const DIALOG_START: &str = "Are you sure you want to accept this trade?";
pub const TRADE_SUCCESS: &str = "The trade was successful!";
const DIALOG_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTrade {
    pub partner: Option<String>,
    pub platinum_given: u32,
    pub platinum_received: u32,
    pub given_items: Vec<TradeItem>,
    pub received_items: Vec<TradeItem>,
    pub log_stamp: Option<String>,
}

#[derive(Default)]
pub struct TradeMachine {
    buffer: Option<Vec<String>>,
    sealed: bool,
    started_ms: u64,
}

impl TradeMachine {
    pub fn feed(&mut self, line: &str, now_ms: u64) -> Option<ParsedTrade> {
        if line.contains(DIALOG_START) {
            self.buffer = Some(vec![line.to_owned()]);
            self.started_ms = now_ms;
            self.sealed = line.contains("leftItem=") || line.contains("rightItem=");
        } else if let Some(buffer) = self.buffer.as_mut() {
            if now_ms.saturating_sub(self.started_ms) > DIALOG_TIMEOUT_MS {
                self.buffer = None;
                self.sealed = false;
            } else if is_framework_line(line) {
                self.sealed = true;
            } else if !self.sealed {
                buffer.push(line.to_owned());
            }
        }

        if line.contains(TRADE_SUCCESS) {
            let buffer = self.buffer.take()?;
            self.sealed = false;
            return parse_trade_dialog(&buffer);
        }
        None
    }
}

pub fn parse_trade_dialog(lines: &[String]) -> Option<ParsedTrade> {
    let text = lines.join("\n");
    let start = text.find("You are offering:")?;
    let description = &text[start..];
    let divider_start = description.find("and will receive from")?;
    let after_divider = &description[divider_start + "and will receive from".len()..];
    let following = after_divider.find("the following:")?;
    let partner = strip_glyphs(after_divider[..following].trim());
    let offering = &description["You are offering:".len()..divider_start];
    let receiving = &after_divider[following + "the following:".len()..];
    let (given_items, platinum_given) = parse_item_block(offering);
    let (received_items, platinum_received) = parse_item_block(receiving);
    let log_stamp = lines
        .first()
        .and_then(|line| line.split_whitespace().next())
        .filter(|stamp| stamp.parse::<f64>().is_ok())
        .map(str::to_owned);

    if given_items.is_empty()
        && received_items.is_empty()
        && platinum_given == 0
        && platinum_received == 0
    {
        return None;
    }
    Some(ParsedTrade {
        partner: (!partner.is_empty()).then_some(partner),
        platinum_given,
        platinum_received,
        given_items,
        received_items,
        log_stamp,
    })
}

fn parse_item_block(block: &str) -> (Vec<TradeItem>, u32) {
    let mut items: Vec<TradeItem> = Vec::new();
    let mut platinum = 0_u32;
    for line in block.lines() {
        let line = line.trim();
        if line.starts_with("leftItem=")
            || line.starts_with("rightItem=")
            || line.starts_with("title=")
        {
            break;
        }
        if is_framework_line(line) {
            continue;
        }
        let cleaned = strip_glyphs(strip_argument_tail(line).trim_end_matches('\r'));
        if cleaned.is_empty() {
            continue;
        }
        if let Some(rest) = cleaned.strip_prefix("Platinum") {
            let rest = rest.trim();
            if rest.is_empty() {
                platinum = platinum.saturating_add(1);
            } else if let Some(value) = rest
                .strip_prefix('x')
                .and_then(|value| value.trim().parse::<u32>().ok())
            {
                platinum = platinum.saturating_add(value);
            }
            continue;
        }
        let (name, quantity) = match cleaned.rsplit_once(" x ") {
            Some((name, quantity)) => quantity.trim().parse::<u32>().ok().map_or_else(
                || (cleaned.clone(), 1),
                |quantity| (name.trim().to_owned(), quantity),
            ),
            None => (cleaned.clone(), 1),
        };
        if name.is_empty() || quantity == 0 {
            continue;
        }
        if let Some(existing) = items.iter_mut().find(|item| item.name == name) {
            existing.quantity = existing.quantity.saturating_add(quantity);
        } else {
            items.push(TradeItem { name, quantity });
        }
    }
    (items, platinum)
}

fn strip_glyphs(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !(('\u{e000}'..='\u{f8ff}').contains(character)
                || ('\u{f0000}'..='\u{ffffd}').contains(character))
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

fn strip_argument_tail(line: &str) -> &str {
    let mut end = line.len();
    for key in [
        ", leftItem=",
        " leftItem=",
        ", rightItem=",
        " rightItem=",
        ", title=",
        " title=",
    ] {
        if let Some(index) = line.find(key) {
            end = end.min(index);
        }
    }
    &line[..end]
}

fn is_framework_line(line: &str) -> bool {
    let mut parts = line.splitn(3, ' ');
    let Some(stamp) = parts.next() else {
        return false;
    };
    if stamp.parse::<f64>().is_err() {
        return false;
    }
    let rest = parts.collect::<Vec<_>>().join(" ");
    rest.contains("[Info]") || rest.contains("[Error]") || rest.contains("[Warning]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sale_dialog() -> String {
        "1234.567 Sys [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Are you sure you want to accept this trade?\nYou are offering:\nPrimed Flow\nLith C5 Relic x 3\nand will receive from SomeTenno\u{e000} the following:\nPlatinum x 45, leftItem=/Menu/Confirm_Item_Ok)".to_owned()
    }

    #[test]
    fn parses_confirmed_sale_with_stacks_and_platform_glyph() {
        let mut machine = TradeMachine::default();
        assert!(machine.feed(&sale_dialog(), 0).is_none());
        let trade = machine
            .feed("1235.0 Sys [Info]: The trade was successful!", 200)
            .expect("confirmed trade");
        assert_eq!(trade.partner.as_deref(), Some("SomeTenno"));
        assert_eq!(trade.platinum_received, 45);
        assert_eq!(trade.platinum_given, 0);
        assert_eq!(
            trade.given_items,
            vec![
                TradeItem {
                    name: "Primed Flow".into(),
                    quantity: 1
                },
                TradeItem {
                    name: "Lith C5 Relic".into(),
                    quantity: 3
                },
            ]
        );
        assert_eq!(trade.log_stamp.as_deref(), Some("1234.567"));
    }

    #[test]
    fn repeated_item_lines_are_merged() {
        let trade = parse_trade_dialog(&[
            "10.0 Sys [Info]: Dialog(description=Are you sure you want to accept this trade?"
                .into(),
            "You are offering:".into(),
            "Platinum x 20".into(),
            "and will receive from Seller the following:".into(),
            "Ash Prime Blueprint".into(),
            "Ash Prime Blueprint".into(),
        ])
        .expect("trade");
        assert_eq!(trade.platinum_given, 20);
        assert_eq!(
            trade.received_items,
            vec![TradeItem {
                name: "Ash Prime Blueprint".into(),
                quantity: 2
            }]
        );
    }

    #[test]
    fn machine_requires_success_and_discards_stale_dialog() {
        let mut machine = TradeMachine::default();
        assert!(machine.feed(&sale_dialog(), 0).is_none());
        assert!(machine.feed("other", 3 * 60 * 1_000).is_none());
        assert!(
            machine
                .feed(
                    "1236.0 Sys [Info]: The trade was successful!",
                    3 * 60 * 1_000 + 1
                )
                .is_none()
        );
    }
}
