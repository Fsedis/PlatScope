use crate::{
    AccountOrder, AccountOrderType, AccountService, CoreError, UpdateListingInput,
    require_write_confirmation,
};

impl AccountService {
    /// Изменяет объявление, только если ожидаемая строка ещё актуальна.
    ///
    /// Проверка и запись держат общий замок аккаунта, поэтому другая локальная
    /// операция не может изменить количество между чтением и PATCH.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] без подтверждения, при изменённом объявлении,
    /// отсутствии авторизации или ошибке WFM.
    pub async fn update_listing_if_unchanged(
        &self,
        id: &str,
        input: &UpdateListingInput,
        expected: Option<&AccountOrder>,
        confirmed: bool,
    ) -> Result<AccountOrder, CoreError> {
        require_write_confirmation(confirmed)?;
        input.validate()?;
        let _operation_guard = self.operation_lock.lock().await;
        let token = self.require_token()?;
        if let Some(expected) = expected {
            let current = self.client.my_orders(&token).await?;
            validate_expected_order(id, expected, &current)?;
        }
        Ok(self.client.update_order(&token, id, input).await?)
    }

    /// Удаляет объявление, только если ожидаемая строка ещё актуальна.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] без подтверждения, при изменённом объявлении,
    /// отсутствии авторизации или ошибке WFM.
    pub async fn delete_listing_if_unchanged(
        &self,
        id: &str,
        expected: Option<&AccountOrder>,
        confirmed: bool,
    ) -> Result<AccountOrder, CoreError> {
        require_write_confirmation(confirmed)?;
        let _operation_guard = self.operation_lock.lock().await;
        let token = self.require_token()?;
        if let Some(expected) = expected {
            let current = self.client.my_orders(&token).await?;
            validate_expected_order(id, expected, &current)?;
        }
        Ok(self.client.delete_order(&token, id).await?)
    }

    /// Проверяет, что все объявления выбранного типа совпадают с показанным снимком.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при добавлении, удалении или изменении объявления.
    pub fn validate_visibility_snapshot(
        expected_orders: &[AccountOrder],
        current_orders: &[AccountOrder],
        order_type: AccountOrderType,
    ) -> Result<(), CoreError> {
        validate_visibility_snapshot(expected_orders, current_orders, order_type)
    }

    /// Меняет видимость одного типа объявлений после проверки снимка, показанного пользователю.
    ///
    /// Проверка и запись используют общий замок операций аккаунта. Для показа продаж
    /// вызывающая сторона предварительно проверяет инвентарь и совместные резервы.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] без подтверждения, при смене объявлений во время
    /// проверки, отсутствии авторизации либо ошибке WFM.
    pub async fn set_orders_visibility(
        &self,
        order_type: AccountOrderType,
        visible: bool,
        expected_orders: &[AccountOrder],
        confirmed: bool,
    ) -> Result<u32, CoreError> {
        require_write_confirmation(confirmed)?;
        let _operation_guard = self.operation_lock.lock().await;
        let token = self.require_token()?;
        let current_orders = self.client.my_orders(&token).await?;
        Self::validate_visibility_snapshot(expected_orders, &current_orders, order_type)?;
        if !current_orders
            .iter()
            .any(|order| order.order_type == order_type && order.visible != visible)
        {
            return Ok(0);
        }
        Ok(self
            .client
            .set_order_group_visibility(&token, order_type, visible)
            .await?
            .updated)
    }
}

fn validate_expected_order(
    id: &str,
    expected: &AccountOrder,
    current: &[AccountOrder],
) -> Result<(), CoreError> {
    if expected.id != id
        || !current
            .iter()
            .any(|order| order.id == id && order == expected)
    {
        return Err(CoreError::AccountData(
            "Объявление уже изменилось или было удалено. Обновите список и проверьте изменения заново."
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_visibility_snapshot(
    expected: &[AccountOrder],
    current: &[AccountOrder],
    order_type: AccountOrderType,
) -> Result<(), CoreError> {
    let expected = expected
        .iter()
        .filter(|order| order.order_type == order_type)
        .collect::<Vec<_>>();
    let current = current
        .iter()
        .filter(|order| order.order_type == order_type)
        .collect::<Vec<_>>();
    if expected.len() != current.len()
        || expected.iter().any(|before| {
            !current
                .iter()
                .any(|now| now.id == before.id && now == before)
        })
    {
        return Err(CoreError::AccountData(
            "Объявления изменились во время проверки. Обновите список и повторите действие."
                .to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn order(id: &str, side: AccountOrderType) -> AccountOrder {
        AccountOrder {
            id: id.to_owned(),
            item_id: Some("item".to_owned()),
            order_type: side,
            platinum: 10,
            quantity: 2,
            per_trade: None,
            rank: None,
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            visible: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn visibility_snapshot_rejects_changes_on_target_side() {
        let before = vec![order("sell", AccountOrderType::Sell)];
        let mut after = before.clone();
        after[0].quantity = 1;
        assert!(validate_visibility_snapshot(&before, &after, AccountOrderType::Sell).is_err());
        after = before.clone();
        after.push(order("new-sell", AccountOrderType::Sell));
        assert!(validate_visibility_snapshot(&before, &after, AccountOrderType::Sell).is_err());
        assert!(validate_visibility_snapshot(&before, &[], AccountOrderType::Sell).is_err());
    }

    #[test]
    fn visibility_snapshot_allows_reordering_and_unrelated_buy_changes() {
        let before = vec![
            order("sell-1", AccountOrderType::Sell),
            order("sell-2", AccountOrderType::Sell),
            order("buy", AccountOrderType::Buy),
        ];
        let mut after = before.clone();
        after[2].quantity = 8;
        after.reverse();
        assert!(validate_visibility_snapshot(&before, &after, AccountOrderType::Sell).is_ok());
        assert!(validate_visibility_snapshot(&before, &after, AccountOrderType::Buy).is_err());
    }

    #[test]
    fn next_batch_write_rejects_quantity_changed_by_a_sale_after_initial_review() {
        let reviewed = vec![
            order("first", AccountOrderType::Sell),
            order("second", AccountOrderType::Sell),
        ];
        // Общая проверка перед циклом прошла. Первая строка сохранена, затем
        // автоматическая продажа уменьшила вторую до начала её PATCH.
        assert!(validate_visibility_snapshot(&reviewed, &reviewed, AccountOrderType::Sell).is_ok());
        let mut current = reviewed.clone();
        current[0].platinum = 11;
        current[1].quantity = 1;
        // Проверяем и количество, а не только updatedAt: серверная точность
        // времени может не различить близкие изменения одной секунды.
        assert_eq!(current[1].updated_at, reviewed[1].updated_at);
        assert!(validate_expected_order("second", &reviewed[1], &current).is_err());
        // После нового просмотра и подтверждения актуальная строка принимается.
        assert!(validate_expected_order("second", &current[1], &current).is_ok());
    }

    #[test]
    fn delete_snapshot_rejects_missing_replaced_or_changed_order() {
        let expected = order("original", AccountOrderType::Sell);
        assert!(validate_expected_order("original", &expected, &[]).is_err());
        let mut replaced = expected.clone();
        replaced.id = "replacement".to_owned();
        assert!(validate_expected_order("replacement", &expected, &[replaced]).is_err());
        let mut changed = expected.clone();
        changed.visible = true;
        assert!(validate_expected_order("original", &expected, &[changed]).is_err());
        assert!(
            validate_expected_order("original", &expected, std::slice::from_ref(&expected)).is_ok()
        );
    }
}
