using System.Text.Json;

namespace PlatScope.RewardOcr;

internal static partial class Program
{
    // Реальная последовательность: открытие → через 5,6 с готовые награды → дубли.
    // Проверка без ожиданий, захвата экрана, игры и OCR.
    private static bool RunRewardTriggerRegression()
    {
        var output = Console.Out;
        using var captured = new StringWriter();
        try
        {
            Console.SetOut(captured);
            var state = new RewardWatcherState();
            var opening = DbwinRewardWatcher.IsRewardMarker("VoidProjections: OpenVoidProjectionRewardScreen");
            if (opening) state.TryEmitReward("dbwin");
            var ready = DbwinRewardWatcher.IsRewardMarker("ProjectionRewardChoice.lua: Got rewards")
                && state.TryEmitReward("dbwin");
            var duplicate = DbwinRewardWatcher.IsRewardMarker("ProjectionRewardChoice.lua: Missing icon data!")
                && state.TryEmitReward("dbwin");
            state.ClearProjections();
            var nextRound = state.TryEmitReward("dbwin");
            var count = captured.ToString().Split('\n', StringSplitOptions.RemoveEmptyEntries).Length;
            var ok = !opening && ready && !duplicate && nextRound && count == 2;
            if (!ok) Console.Error.WriteLine("Ошибка запуска OCR: ранний маркер, готовность или подавление дублей.");
            return ok && RunRelicSelectionTriggerRegression();
        }
        finally { Console.SetOut(output); }
    }

    private static bool RunRelicSelectionTriggerRegression()
    {
        const string create = "Sys [Info]: Created /Lotus/Interface/ThemedProjectionManager.swf";
        const string dialog = "Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Вы уверены, что хотите взять Реликвия Лит T14 с собой на миссию? В случае запечатывания Разрыв Бездны и эвакуации предмет будет использован., title= leftItem=/Menu/Confirm_Item_Yes, rightItem=/Menu/Confirm_Item_No)";
        const string owner = "1234567890abcdef12345678";
        const string anotherOwner = "abcdef1234567890abcdef12";
        const string ownReward = "Sys [Info]: VoidProjections: " + owner + " gets reward /Lotus/StoreItems/Types/Recipes/WarframeRecipes/LavosPrimeChassisBlueprint";
        const string sentReward = "Sys [Info]: VoidProjections: Sending reward info from " + owner + " to host!";
        var time = DateTimeOffset.Parse("2026-09-12T00:00:00Z");
        var events = new List<RelicSelectionEvent>();
        var state = new RelicSelectionWatcher(events.Add, () => time);
        var checks = new List<bool>();
        void Observe(string line) => state.Observe(42, line);
        int Count(string phase) => events.Count(value => value.Phase == phase);

        Observe("ResourceLoader (/Lotus/Interface/ThemedProjectionManager.swf) Found 5746 items to load");
        checks.Add(Count("open") == 0);
        Observe(create);
        Observe(create);
        checks.Add(Count("open") == 1 && events.Last().Era is null);
        Observe("Net [Info]: Set squad mission: {\"difficulty\":\"\",\"voidTier\":\"VoidT1\",\"name\":\"SolNode123_ActiveMission\"}");
        checks.Add(events.Last().Phase == "mission" && events.Last().Era == "lith"
            && events.Last().MissionNode == "SolNode123_ActiveMission");
        Observe(ownReward);
        Observe(sentReward);
        Observe("ProjectionRewardChoice.lua: Got rewards");
        checks.Add(Count("opened") == 0); // Чужие награды и Got rewards сами не расходуют реликвию.

        Observe(dialog);
        checks.Add(events.Last().RelicName == "Lith T14" && events.Last().Refinement is null);
        Observe("Dialog.lua: Dialog::SendResult(0)");
        Observe(ownReward);
        Observe(sentReward);
        checks.Add(Count("selected") == 0 && Count("opened") == 0);

        time += TimeSpan.FromSeconds(3);
        Observe(dialog);
        Observe("Dialog.lua: SendResult_MENU_SELECT()");
        Observe("Dialog.lua: Dialog::SendResult(4)");
        Observe("Dialog.lua: Dialog::SendResult(4)");
        var selected = events.Last(value => value.Phase == "selected");
        checks.Add(Count("selected") == 1 && selected.SelectionId == 2 && selected.Refinement is null);
        Observe("ProjectionRewardChoice.lua: Got rewards");
        Observe(ownReward);
        Observe("VoidProjections: Sending reward info from " + anotherOwner + " to host!");
        checks.Add(Count("opened") == 0);
        Observe(sentReward);
        Observe(sentReward);
        Observe(ownReward);
        Observe(sentReward);
        var opened = events.Last(value => value.Phase == "opened");
        checks.Add(Count("opened") == 1 && opened.SelectionId == selected.SelectionId
            && opened.RelicName == "Lith T14" && opened.Refinement is null);
        var encoded = JsonSerializer.Serialize(opened);
        checks.Add(encoded.Contains("\"type\":\"relic_selection\"", StringComparison.Ordinal)
            && encoded.Contains("\"sessionId\":", StringComparison.Ordinal)
            && !encoded.Contains(owner, StringComparison.Ordinal));

        time += TimeSpan.FromSeconds(3);
        Observe(create);
        Observe(dialog);
        Observe("Dialog.lua: Dialog::CreateOkCancel(description=Другой вопрос, title= leftItem=/Menu/Confirm_Item_Yes, rightItem=/Menu/Confirm_Item_No)");
        Observe("Dialog.lua: Dialog::SendResult(4)");
        checks.Add(Count("selected") == 1); // Подтверждение нового диалога не подтверждает старую реликвию.
        time += TimeSpan.FromSeconds(3);
        Observe(dialog);
        time += TimeSpan.FromSeconds(31);
        Observe("Dialog.lua: Dialog::SendResult(4)");
        checks.Add(Count("selected") == 1);

        time += TimeSpan.FromSeconds(3);
        Observe("Net [Info]: Set squad mission: {\"difficulty\":\"\",\"voidTier\":\"VoidT6\",\"name\":\"SolNode717_ActiveMission\"}");
        checks.Add(events.Last().Era == "omnia");
        Observe(dialog.Replace("Реликвия Лит T14", "Реликвия Акси A1 (Сияющая)"));
        checks.Add(events.Last().Era == "omnia" && events.Last().RelicName == "Axi A1");
        Observe("Dialog.lua: Dialog::SendResult(4)");
        checks.Add(events.Last().Era == "omnia" && events.Last().RelicName == "Axi A1"
            && events.Last().Refinement == "radiant");
        Observe(ownReward);
        time += TimeSpan.FromSeconds(11);
        Observe(sentReward);
        checks.Add(Count("opened") == 1); // Старое сообщение другого раунда не списывает копию.

        var previousSession = events.Last().SessionId;
        state.Observe(43, sentReward);
        checks.Add(events.Last().Phase == "reset" && events.Last().SessionId != previousSession
            && events.Last().SelectionId == 0 && Count("opened") == 1);
        state.ObserveVisibility(true);
        state.ObserveVisibility(true);
        state.ObserveVisibility(false);
        checks.Add(Count("visibility") == 2 && events.Last().Foreground == false);
        Observe("Net [Info]: Set squad mission: {broken json}");

        // Подготовка до диалога: OCR меняет сведения просмотра, но не подтверждает
        // выбор и не расходует копии. Поздний кадр после выбора не открывает окно.
        var preparationEvents = new List<RelicSelectionEvent>();
        var preparation = new RelicSelectionWatcher(preparationEvents.Add, () => time);
        preparation.Observe(42, create);
        preparation.ObserveVisibility(true);
        checks.Add(preparation.TryGetPreparationContext(out var preparationPid, out var frameGeneration));
        preparation.ObservePreparation(preparationPid, frameGeneration, new RelicPreparationReading("neo", "Neo T11", null));
        checks.Add(preparationEvents.Last().Phase == "browse" && preparationEvents.Last().Era == "neo"
            && preparationEvents.Last().ScreenEra == "neo" && preparationEvents.Last().RelicName == "Neo T11"
            && preparationEvents.Last().Refinement is null && preparationEvents.Last().Source == "screen_ocr");
        var unchanged = preparationEvents.Count;
        preparation.ObservePreparation(preparationPid, frameGeneration, new RelicPreparationReading("neo", "Neo T11", null));
        preparation.Observe(42, ownReward);
        preparation.Observe(42, sentReward);
        checks.Add(preparationEvents.Count == unchanged);
        preparation.Observe(42, "Net [Info]: Set squad mission: {\"voidTier\":\"VoidT6\",\"name\":\"SolNode717_ActiveMission\"}");
        checks.Add(preparation.TryGetPreparationContext(out preparationPid, out frameGeneration));
        preparation.ObservePreparation(preparationPid, frameGeneration, new RelicPreparationReading("neo", "Neo T11", null));
        checks.Add(preparationEvents.Last().Era == "omnia" && preparationEvents.Last().ScreenEra == "neo");
        preparation.Observe(42, dialog.Replace("Реликвия Лит T14", "Реликвия Нео T11"));
        checks.Add(!preparation.TryGetPreparationContext(out _, out _));
        preparation.Observe(42, "Dialog.lua: Dialog::SendResult(4)");
        unchanged = preparationEvents.Count;
        preparation.ObservePreparation(preparationPid, frameGeneration, new RelicPreparationReading("lith", "Lith T14", null));
        checks.Add(preparationEvents.Count == unchanged && preparationEvents.Last().Phase == "selected"
            && preparationEvents.Last().RelicName == "Neo T11"
            && !preparation.TryGetPreparationRecoveryContext(42, out _));
        preparation.Observe(42, ownReward);
        preparation.Observe(42, sentReward);
        checks.Add(preparationEvents.Count(value => value.Phase == "opened") == 1
            && preparationEvents.Last().RelicName == "Neo T11");

        // Восстановление уже открытой подготовки после запуска помощника.
        var recoveredEvents = new List<RelicSelectionEvent>();
        var recovered = new RelicSelectionWatcher(recoveredEvents.Add, () => time);
        recovered.ObserveVisibility(true);
        checks.Add(recovered.TryGetPreparationRecoveryContext(43, out var recoveryGeneration));
        recovered.ObservePreparation(43, recoveryGeneration, new RelicPreparationReading("neo", "Neo T11", null), recovery: true);
        checks.Add(recoveredEvents.Count(value => value.Phase == "open") == 1
            && recoveredEvents.Last().Phase == "browse"
            && recoveredEvents.All(value => value.Phase is not "selected" and not "opened"));
        checks.Add(recovered.TryGetPreparationContext(out var recoveryPid, out recoveryGeneration));
        recovered.ObservePreparationNotVisible(recoveryPid, recoveryGeneration);
        recovered.ObservePreparationNotVisible(recoveryPid, recoveryGeneration);
        checks.Add(recoveredEvents.Last().Phase == "browse");
        recovered.ObservePreparationNotVisible(recoveryPid, recoveryGeneration);
        unchanged = recoveredEvents.Count;
        recovered.ObservePreparation(recoveryPid, recoveryGeneration, new RelicPreparationReading("neo", "Neo T11", null), recovery: true);
        checks.Add(recoveredEvents.Count == unchanged && recoveredEvents.Last().Phase == "closed");
        checks.Add(recovered.TryGetPreparationRecoveryContext(43, out var freshRecoveryGeneration));
        recovered.ObservePreparation(43, freshRecoveryGeneration, new RelicPreparationReading("neo", "Neo T11", null), recovery: true);
        checks.Add(recoveredEvents.Count(value => value.Phase == "open") == 2
            && recoveredEvents.Last().Phase == "browse"
            && recoveredEvents.All(value => value.Phase is not "selected" and not "opened"));

        var ok = checks.All(value => value);
        if (!ok) Console.Error.WriteLine("Ошибка выбора реликвии: подтверждение, собственное раскрытие, дубли или смена сессии.");
        return ok;
    }
}
