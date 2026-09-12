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
            return ok;
        }
        finally { Console.SetOut(output); }
    }
}
