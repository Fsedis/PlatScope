using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;

namespace PlatScope.RewardOcr;

// Просматриваемую реликвию и фильтр эры читаем со снимка подготовки.
// Подтверждение собственного выбора и расход берём только из доказательных
// сообщений игры: загрузки ресурсов относятся также к реликвиям отряда.
internal sealed class RelicSelectionWatcher
{
    private static readonly TimeSpan ConfirmationWindow = TimeSpan.FromSeconds(30);
    private static readonly TimeSpan OwnRewardWindow = TimeSpan.FromSeconds(10);
    private readonly Action<RelicSelectionEvent> emit;
    private readonly Func<DateTimeOffset> now;
    private readonly object gate = new();
    private readonly Dictionary<string, DateTimeOffset> rewardOwners = new(StringComparer.Ordinal);
    private int processId;
    private string sessionId = Guid.NewGuid().ToString("N");
    private ulong selectionId;
    private bool visible;
    private bool confirmed;
    private bool opened;
    private string? era;
    private string? relicName;
    private string? refinement;
    private string? missionNode;
    private string? missionName;
    private string? missionTier;
    private string? screenEra;
    private ulong preparationGeneration;
    private bool preparationRecoveryAllowed = true;
    private bool preparationHeaderSeen;
    private int missingPreparationHeaders;
    private PendingRelic? pending;
    private bool? lastForeground;
    private string? lastConfirmationLine;
    private DateTimeOffset lastConfirmationAt;
    private string? lastCreatedLine;
    private DateTimeOffset lastCreatedAt;

    internal RelicSelectionWatcher(Action<RelicSelectionEvent> emit, Func<DateTimeOffset>? now = null)
    {
        this.emit = emit;
        this.now = now ?? (() => DateTimeOffset.UtcNow);
    }

    internal void Observe(int sourceProcessId, string line)
    {
        lock (gate) ObserveLocked(sourceProcessId, line);
    }

    private void ObserveLocked(int sourceProcessId, string line)
    {
        if (sourceProcessId != processId)
        {
            processId = sourceProcessId;
            ResetSession("process_changed");
        }

        var observedAt = now();
        if (pending is not null && observedAt - pending.ObservedAt > ConfirmationWindow) pending = null;
        foreach (var owner in rewardOwners.Where(pair => observedAt - pair.Value > OwnRewardWindow)
                     .Select(pair => pair.Key).ToArray()) rewardOwners.Remove(owner);

        // Created — реальное открытие интерфейса. ResourceLoader/Spot-loading
        // того же SWF только подготавливает файл и не открывает окно.
        if (line.Contains("Created /Lotus/Interface/ThemedProjectionManager.swf", StringComparison.Ordinal))
        {
            if (!IsDuplicate(line, lastCreatedLine, observedAt - lastCreatedAt))
                OpenSelection("manager_created");
            lastCreatedLine = line;
            lastCreatedAt = observedAt;
        }

        if (line.Contains("Dialog.lua: Dialog::Create", StringComparison.Ordinal))
        {
            // Любой новый диалог аннулирует ожидаемое подтверждение старого.
            var parsed = TryParseOwnSelection(line, observedAt);
            var duplicate = IsDuplicate(line, lastConfirmationLine, observedAt - lastConfirmationAt);
            if (parsed is not null && !duplicate)
            {
                if (!visible) OpenSelection("selection_dialog");
                preparationGeneration++;
                pending = parsed;
                screenEra = pending.Era;
                era = TierEra(missionTier) ?? pending.Era;
                relicName = pending.RelicName;
                refinement = pending.Refinement;
                lastConfirmationLine = line;
                lastConfirmationAt = observedAt;
                Publish("open", "selection_dialog");
            }
            else if (!duplicate)
            {
                pending = null;
                if (!confirmed)
                {
                    relicName = null;
                    refinement = null;
                }
            }
        }

        var dialogResult = Regex.Match(line, @"Dialog\.lua: Dialog::SendResult\(([0-9]+)\)");
        if (dialogResult.Success && pending is not null)
        {
            if (dialogResult.Groups[1].Value == "4")
            {
                era = TierEra(missionTier) ?? pending.Era;
                relicName = pending.RelicName;
                refinement = pending.Refinement;
                confirmed = true;
                opened = false;
                rewardOwners.Clear();
                Publish("selected", "selection_confirmed");
                visible = false;
                preparationGeneration++;
            }
            else
            {
                relicName = null;
                refinement = null;
                confirmed = false;
                Publish("open", "selection_cancelled");
            }
            pending = null;
        }
        else if (line.Contains("Dialog.lua: SendResult_MENU_CANCEL()", StringComparison.Ordinal))
        {
            pending = null;
            if (visible)
            {
                relicName = null;
                refinement = null;
                Publish("open", "selection_cancelled");
            }
        }

        if (TryReadMission(line, out var node, out var tier))
        {
            var changed = missionNode != node || missionTier != tier;
            missionNode = node;
            missionTier = tier;
            if (changed)
            {
                preparationGeneration++;
                missionName = null;
            }
            if (TierEra(missionTier) is { } missionEra) era = missionEra;
            else if (!confirmed && pending is null) era = null;
            Publish("mission", "mission_context");
            if (line.Contains("Requested mission:", StringComparison.Ordinal)
                || line.Contains("Client loaded {", StringComparison.Ordinal)) CloseSelection("mission_started");
        }
        const string missionNamePrefix = "ThemedSquadOverlay.lua: Cached mission name=";
        var missionNameAt = line.IndexOf(missionNamePrefix, StringComparison.Ordinal);
        if (missionNameAt >= 0)
        {
            var value = line[(missionNameAt + missionNamePrefix.Length)..].Trim();
            if (value.Length is > 0 and <= 256)
            {
                missionName = value;
                Publish("mission", "mission_name");
            }
        }

        // Эта пара найдена в локальной записи успешного раскрытия собственного
        // предмета. Идентификатор игрока живёт только в коротком внутреннем окне
        // сопоставления и никогда не попадает в событие или журнал помощника.
        var ownReward = Regex.Match(line,
            @"VoidProjections: ([a-fA-F0-9]{24}) gets reward /Lotus/StoreItems/[A-Za-z0-9/_]+(?:\s|$)");
        if (ownReward.Success && confirmed && !opened)
        {
            if (rewardOwners.Count < 8) rewardOwners[ownReward.Groups[1].Value] = observedAt;
        }
        var sentReward = Regex.Match(line,
            @"VoidProjections: Sending reward info from ([a-fA-F0-9]{24}) to host!");
        if (sentReward.Success && confirmed && !opened
            && rewardOwners.TryGetValue(sentReward.Groups[1].Value, out var receivedAt)
            && observedAt - receivedAt <= OwnRewardWindow)
        {
            opened = true;
            visible = false;
            preparationGeneration++;
            pending = null;
            rewardOwners.Clear();
            Publish("opened", "own_reward_confirmed");
        }

        if (line.Contains("VoidProjections: OpenVoidProjectionRewardScreen", StringComparison.Ordinal)
            || DbwinRewardWatcher.IsRewardMarker(line)) CloseSelection("reward_screen", "reward");

        if (line.Contains("Destroyed /Lotus/Interface/ThemedProjectionManager.swf", StringComparison.Ordinal)
            || line.Contains("ThemedProjectionManager.lua: Shutdown", StringComparison.Ordinal))
            CloseSelection("manager_closed");

        if (line.Contains("EndOfMatch.lua: Initialize", StringComparison.Ordinal)
            || line.Contains("Loading /Lotus/Levels/Proc/PlayerShip/", StringComparison.Ordinal))
        {
            CloseSelection("mission_ended");
            pending = null;
            confirmed = false;
            opened = false;
            rewardOwners.Clear();
            relicName = null;
            refinement = null;
            era = null;
            missionNode = null;
            missionName = null;
            missionTier = null;
            screenEra = null;
            preparationGeneration++;
            Publish("mission", "mission_ended");
        }
    }

    internal void ObserveVisibility(bool foreground)
    {
        lock (gate) ObserveVisibilityLocked(foreground);
    }

    private void ObserveVisibilityLocked(bool foreground)
    {
        if (lastForeground == foreground) return;
        preparationGeneration++;
        lastForeground = foreground;
        emit(new RelicSelectionEvent("visibility", sessionId, selectionId, era, relicName,
            refinement, missionNode, missionName, missionTier, "dbwin", "foreground_changed", foreground, screenEra));
    }

    internal bool TryGetPreparationContext(out int sourceProcessId, out ulong generation)
    {
        lock (gate)
        {
            sourceProcessId = processId;
            generation = preparationGeneration;
            return visible && !confirmed && pending is null && lastForeground == true && processId > 0;
        }
    }

    internal bool TryGetPreparationRecoveryContext(int sourceProcessId, out ulong generation)
    {
        lock (gate)
        {
            if (sourceProcessId > 0 && sourceProcessId != processId)
            {
                processId = sourceProcessId;
                ResetSession("foreground_process");
                ObserveVisibilityLocked(true);
            }
            generation = preparationGeneration;
            return sourceProcessId == processId && sourceProcessId > 0 && preparationRecoveryAllowed
                && !visible && !confirmed && pending is null && lastForeground == true;
        }
    }

    internal void ObservePreparation(int sourceProcessId, ulong generation, RelicPreparationReading reading,
        bool recovery = false)
    {
        lock (gate)
        {
            // Кадр мог быть снят до подтверждения, закрытия или следующего открытия.
            // Его завершившийся позже OCR не должен менять новое состояние.
            if (sourceProcessId != processId || generation != preparationGeneration
                || confirmed || pending is not null || lastForeground != true) return;
            if (recovery)
            {
                if (!preparationRecoveryAllowed || visible) return;
                OpenSelection("preparation_recovered");
            }
            else if (!visible) return;
            preparationHeaderSeen = true;
            missingPreparationHeaders = 0;
            var nextScreenEra = reading.Era is "lith" or "meso" or "neo" or "axi" or "requiem"
                ? reading.Era : null;
            var nextEra = TierEra(missionTier) ?? nextScreenEra;
            if (screenEra == nextScreenEra && era == nextEra
                && relicName == reading.RelicName && refinement == reading.Refinement) return;
            screenEra = nextScreenEra;
            era = nextEra;
            relicName = reading.RelicName;
            refinement = reading.Refinement;
            Publish("browse", "preparation_visible", "screen_ocr");
        }
    }

    internal void ObservePreparationNotVisible(int sourceProcessId, ulong generation)
    {
        lock (gate)
        {
            if (sourceProcessId != processId || generation != preparationGeneration
                || !visible || confirmed || pending is not null || lastForeground != true
                || !preparationHeaderSeen) return;
            if (++missingPreparationHeaders >= 3) CloseSelection("preparation_not_visible");
        }
    }

    private void ResetSession(string reason)
    {
        sessionId = $"{processId}:{Guid.NewGuid():N}";
        selectionId = 0;
        visible = false;
        confirmed = false;
        opened = false;
        pending = null;
        era = null;
        relicName = null;
        refinement = null;
        missionNode = null;
        missionName = null;
        missionTier = null;
        screenEra = null;
        preparationGeneration++;
        preparationRecoveryAllowed = true;
        preparationHeaderSeen = false;
        missingPreparationHeaders = 0;
        lastForeground = null;
        lastConfirmationLine = null;
        lastCreatedLine = null;
        rewardOwners.Clear();
        Publish("reset", reason);
    }

    private void OpenSelection(string reason)
    {
        if (visible) return;
        selectionId++;
        preparationGeneration++;
        preparationRecoveryAllowed = false;
        preparationHeaderSeen = false;
        missingPreparationHeaders = 0;
        visible = true;
        confirmed = false;
        opened = false;
        pending = null;
        relicName = null;
        refinement = null;
        screenEra = null;
        era = TierEra(missionTier);
        rewardOwners.Clear();
        Publish("open", reason);
    }

    private void CloseSelection(string reason, string phase = "closed")
    {
        if (!visible) return;
        visible = false;
        preparationGeneration++;
        preparationRecoveryAllowed = reason == "preparation_not_visible";
        pending = null;
        Publish(phase, reason);
    }

    private void Publish(string phase, string reason, string source = "dbwin") => emit(new RelicSelectionEvent(
        phase, sessionId, selectionId, era, relicName, refinement,
        missionNode, missionName, missionTier, source, reason, null, screenEra));

    private static PendingRelic? TryParseOwnSelection(string line, DateTimeOffset observedAt)
    {
        if (!line.Contains("leftItem=/Menu/Confirm_Item_Yes", StringComparison.Ordinal)
            || !line.Contains("rightItem=/Menu/Confirm_Item_No", StringComparison.Ordinal)
            || !line.Contains("хотите взять", StringComparison.OrdinalIgnoreCase)
            || !line.Contains("с собой на миссию", StringComparison.OrdinalIgnoreCase)) return null;
        var name = Regex.Match(line,
            @"Реликвия\s+(Лит|Мезо|Нео|Акси|Реквием)\s+([A-Za-z][0-9]{1,4}|[IVX]{1,5})(?:\s|[?(])",
            RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);
        if (!name.Success) return null;
        var (era, canonicalEra) = name.Groups[1].Value.ToLowerInvariant() switch
        {
            "лит" => ("lith", "Lith"),
            "мезо" => ("meso", "Meso"),
            "нео" => ("neo", "Neo"),
            "акси" => ("axi", "Axi"),
            "реквием" => ("requiem", "Requiem"),
            _ => (null, null),
        };
        if (era is null) return null;
        // Без явного слова улучшения диалог не различает варианты. Не считаем
        // отсутствие слова доказательством нетронутой реликвии.
        var descriptionStart = line.IndexOf("description=", StringComparison.Ordinal);
        var descriptionEnd = line.IndexOf(", title=", StringComparison.Ordinal);
        var description = descriptionStart >= 0 && descriptionEnd > descriptionStart
            ? line[descriptionStart..descriptionEnd]
            : string.Empty;
        var refinements = new[]
        {
            (Word: "Нетронутая", Refinement: "intact"),
            (Word: "Исключительная", Refinement: "exceptional"),
            (Word: "Безупречная", Refinement: "flawless"),
            (Word: "Сияющая", Refinement: "radiant"),
        }.Where(value => Regex.IsMatch(description, $@"\b{value.Word}\b",
            RegexOptions.IgnoreCase | RegexOptions.CultureInvariant)).ToArray();
        return new PendingRelic(era, $"{canonicalEra} {name.Groups[2].Value.ToUpperInvariant()}",
            refinements.Length == 1 ? refinements[0].Refinement : null, observedAt);
    }

    private static bool TryReadMission(string line, out string? node, out string? tier)
    {
        node = null;
        tier = null;
        if (!line.Contains("Set squad mission:", StringComparison.Ordinal)
            && !line.Contains("Requested mission:", StringComparison.Ordinal)
            && !line.Contains("Client loaded {", StringComparison.Ordinal)) return false;
        var start = line.IndexOf('{');
        var end = line.IndexOf('}', start >= 0 ? start : 0);
        if (start < 0 || end <= start) return false;
        try
        {
            using var document = JsonDocument.Parse(line[start..(end + 1)]);
            var root = document.RootElement;
            if (root.TryGetProperty("name", out var name) && name.ValueKind == JsonValueKind.String)
            {
                var value = name.GetString();
                if (!string.IsNullOrEmpty(value) && value.Length <= 128
                    && value.All(character => char.IsAsciiLetterOrDigit(character) || character == '_')) node = value;
            }
            if (root.TryGetProperty("voidTier", out var voidTier) && voidTier.ValueKind == JsonValueKind.String)
            {
                var value = voidTier.GetString();
                if (TierEra(value) is not null) tier = value;
            }
            return true;
        }
        catch (JsonException) { return false; }
    }

    private static string? TierEra(string? tier) => tier switch
    {
        "VoidT1" => "lith", "VoidT2" => "meso", "VoidT3" => "neo",
        "VoidT4" => "axi", "VoidT5" => "requiem", "VoidT6" => "omnia", _ => null,
    };

    private static bool IsDuplicate(string line, string? previous, TimeSpan age) =>
        line == previous && (age < TimeSpan.FromSeconds(2) || Regex.IsMatch(line, @"^\s*[0-9]+\.[0-9]+\s"));

    private sealed record PendingRelic(string Era, string RelicName, string? Refinement, DateTimeOffset ObservedAt);
}

internal sealed record RelicSelectionEvent(
    [property: JsonPropertyName("phase")] string Phase,
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("selectionId")] ulong SelectionId,
    [property: JsonPropertyName("era")] string? Era,
    [property: JsonPropertyName("relicName")] string? RelicName,
    [property: JsonPropertyName("refinement")] string? Refinement,
    [property: JsonPropertyName("missionNode")] string? MissionNode,
    [property: JsonPropertyName("missionName")] string? MissionName,
    [property: JsonPropertyName("missionTier")] string? MissionTier,
    [property: JsonPropertyName("source")] string Source,
    [property: JsonPropertyName("reason")] string Reason,
    [property: JsonPropertyName("foreground")] bool? Foreground = null,
    [property: JsonPropertyName("screenEra")] string? ScreenEra = null)
{
    [JsonPropertyName("type")]
    public string Type => "relic_selection";
}
