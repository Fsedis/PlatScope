using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Tesseract;

namespace PlatScope.RewardOcr;

internal static class Program
{
    private const int ReferenceWidth = 1920;
    private const int ReferenceHeight = 1080;
    private const int RewardWidth = 968;
    private const int RewardHeight = 235;
    private const int RewardYDisplay = 316;
    private const int RewardLineHeight = 48;
    private static readonly int[] RewardSlotLayouts = [4, 3, 2];
    private const int DefaultMaxAttempts = 6;
    private const int DefaultRetryIntervalMs = 250;
    private const int DefaultInitialDelayMs = 300;
    private const int RetryWindowMs = 2_400;
    private const string OcrLanguage = "rus";
    private const string EmbeddedTessdataName = "PlatScope.RewardOcr.rus.traineddata";
    private const string RussianCharacterWhitelist =
        "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ" +
        "абвгдеёжзийклмнопрстуфхцчшщъыьэюя" +
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 -'&:()";

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        PropertyNameCaseInsensitive = true,
    };

    [STAThread]
    private static int Main(string[] args)
    {
        Console.InputEncoding = Encoding.UTF8;
        Console.OutputEncoding = Encoding.UTF8;
        try
        {
            NativeMethods.TryEnablePerMonitorDpiAwareness();
            if (args.FirstOrDefault() == "--watch-warframe-log")
            {
                var parentProcessId = args.Length > 1 && int.TryParse(args[1], out var parsedParent)
                    ? parsedParent
                    : 0;
                WatchRequest? watcherRequest = null;
                if (args.Contains("--visual-fallback"))
                {
                    watcherRequest = JsonSerializer.Deserialize<WatchRequest>(
                        Console.In.ReadToEnd(), JsonOptions);
                }
                return DbwinRewardWatcher.Run(
                    parentProcessId,
                    args.Contains("--allow-any-process"),
                    watcherRequest);
            }
            if (args.FirstOrDefault() == "--emit-debug-line")
            {
                NativeMethods.EmitDebugLine(args.ElementAtOrDefault(1) ?? string.Empty);
                return 0;
            }
            if (args.FirstOrDefault() == "--warframe-window-rect")
            {
                var bounds = FindWarframeWindowBounds();
                Console.Out.Write(JsonSerializer.Serialize(new
                {
                    x = bounds.Left,
                    y = bounds.Top,
                    width = bounds.Width,
                    height = bounds.Height,
                }, JsonOptions));
                return 0;
            }
            if (args.FirstOrDefault() == "--self-test-russian")
            {
                return RunRussianSelfTest();
            }
            var request = JsonSerializer.Deserialize<ScanRequest>(Console.In.ReadToEnd(), JsonOptions)
                ?? throw new InvalidDataException("OCR request is empty.");
            var result = Scan(request);
            Console.Out.Write(JsonSerializer.Serialize(result, JsonOptions));
            return result.Status == "ok" ? 0 : 2;
        }
        catch (Exception error)
        {
            Console.Out.Write(JsonSerializer.Serialize(
                ScanResult.Error("scan_failed", error.Message), JsonOptions));
            return 1;
        }
    }

    private static ScanResult Scan(ScanRequest request)
    {
        if (request.Catalog.Count == 0)
        {
            return ScanResult.Error("catalog_missing", "Каталог предметов пуст.");
        }

        var tessdata = ResolveTessdata(request.TessdataPath);
        var catalog = request.Catalog
            .Where(item => !string.IsNullOrWhiteSpace(item.Name))
            .Select(item => new CatalogCandidate(item, Normalize(item.Name)))
            .ToArray();
        var isLiveCapture = string.IsNullOrWhiteSpace(request.ImagePath);
        var maxAttempts = isLiveCapture
            ? Math.Clamp(request.MaxAttempts ?? DefaultMaxAttempts, 1, DefaultMaxAttempts)
            : 1;
        var retryIntervalMs = Math.Clamp(
            request.RetryIntervalMs ?? DefaultRetryIntervalMs,
            0,
            2_000);
        var initialDelayMs = isLiveCapture
            ? Math.Clamp(request.InitialDelayMs ?? DefaultInitialDelayMs, 0, 2_000)
            : 0;

        using var engine = CreateRussianEngine(tessdata);
        if (initialDelayMs > 0)
        {
            Thread.Sleep(initialDelayMs);
        }

        ScanResult? best = null;
        var stableCompleteFrames = 0;
        var stableSlotCount = 0;
        var scanTimer = Stopwatch.StartNew();
        for (var attempt = 0; attempt < maxAttempts; attempt++)
        {
            try
            {
                using var screenshot = isLiveCapture
                    ? CaptureWarframeWindow()
                    : LoadScreenshot(request.ImagePath!);
                var candidate = ScanFrame(screenshot, request.UiScale, engine, catalog);
                best = ChooseBetterResult(best, candidate);
                if (best.Rewards.Count == 4 && CountMatched(best) == 4)
                {
                    break;
                }

                var candidateMatched = CountMatched(candidate);
                var candidateComplete = candidateMatched >= 2
                    && candidateMatched == candidate.Rewards.Count;
                if (candidateComplete && candidate.Rewards.Count == 4)
                {
                    break;
                }
                if (candidateComplete)
                {
                    stableCompleteFrames = stableSlotCount == candidate.Rewards.Count
                        ? stableCompleteFrames + 1
                        : 1;
                    stableSlotCount = candidate.Rewards.Count;
                    if (stableCompleteFrames >= 3)
                    {
                        break;
                    }
                }
                else
                {
                    stableCompleteFrames = 0;
                    stableSlotCount = 0;
                }
            }
            catch (Exception error)
            {
                best ??= ScanResult.Error("scan_retry", error.Message);
                if (attempt + 1 >= maxAttempts)
                {
                    break;
                }
            }

            var remainingMs = RetryWindowMs - (int)scanTimer.ElapsedMilliseconds;
            if (remainingMs <= 0)
            {
                break;
            }
            if (attempt + 1 < maxAttempts && retryIntervalMs > 0)
            {
                Thread.Sleep(Math.Min(retryIntervalMs, remainingMs));
            }
        }

        return best ?? ScanResult.Error("scan_failed", "Не удалось получить кадр с наградами.");
    }

    private static int RunRussianSelfTest()
    {
        using var screenshot = new Bitmap(
            ReferenceWidth,
            ReferenceHeight,
            PixelFormat.Format32bppArgb);
        using (var graphics = Graphics.FromImage(screenshot))
        using (var textBrush = new SolidBrush(Color.FromArgb(36, 184, 242)))
        using (var probeBrush = new SolidBrush(Color.FromArgb(36, 183, 241)))
        using (var font = new Font("Arial", 15, FontStyle.Bold, GraphicsUnit.Pixel))
        {
            graphics.Clear(Color.FromArgb(39, 53, 96));
            graphics.FillRectangle(probeBrush, 148, 85, 5, 5);
            var labels = new[]
            {
                "Чертёж: Форма",
                "Никс Прайм: Каркас",
                "Ивара Прайм: Система",
                "Локи Прайм: Нейрооптика",
            };
            var left = ReferenceWidth / 2 - RewardWidth / 2;
            var slotWidth = RewardWidth / labels.Length;
            var top = ReferenceHeight / 2
                - (RewardYDisplay - RewardHeight + RewardLineHeight)
                + 10;
            for (var index = 0; index < labels.Length; index++)
            {
                graphics.DrawString(
                    labels[index],
                    font,
                    textBrush,
                    left + index * slotWidth + 8,
                    top);
            }
        }

        var catalog = BuildCandidates(new[]
        {
            new CatalogItem("forma", "forma_blueprint", "Чертёж: Форма"),
            new CatalogItem(
                "nyx",
                "nyx_prime_chassis_blueprint",
                "Никс Прайм: Каркас (Чертеж)"),
            new CatalogItem(
                "ivara",
                "ivara_prime_systems_blueprint",
                "Ивара Прайм: Система (Чертеж)"),
            new CatalogItem(
                "loki",
                "loki_prime_neuroptics_blueprint",
                "Локи Прайм: Нейрооптика (Чертеж)"),
        });
        using var engine = CreateRussianEngine(ResolveTessdata(null));
        var result = ScanFrame(screenshot, 1.0, engine, catalog);
        Console.Out.Write(JsonSerializer.Serialize(result, JsonOptions));
        return result.Status == "ok" && CountMatched(result) == 4 ? 0 : 3;
    }

    private static ScanResult ScanFrame(
        Bitmap screenshot,
        double? requestedUiScale,
        TesseractEngine engine,
        IReadOnlyList<CatalogCandidate> catalog)
    {
        var scale = Math.Min(
            screenshot.Width / (double)ReferenceWidth,
            screenshot.Height / (double)ReferenceHeight);
        if (scale <= 0.2)
        {
            return ScanResult.Error("invalid_capture", "Снимок окна Warframe имеет неверный размер.");
        }

        var theme = DetectTheme(screenshot, scale);
        var uiScale = ResolveUiScale(requestedUiScale);
        using var rewardStrip = CropRewardStrip(screenshot, scale, uiScale);
        var detectedSlotCount = CountPopulatedRewardSections(rewardStrip, theme);
        var slotLayouts = detectedSlotCount is >= 2 and <= 4
            ? [detectedSlotCount]
            : RewardSlotLayouts;
        ScanResult? best = null;
        foreach (var slotCount in slotLayouts)
        {
            var slots = SplitAndFilter(rewardStrip, theme, slotCount);
            var rewards = new List<RewardMatch>(slots.Count);
            for (var index = 0; index < slots.Count; index++)
            {
                using var slot = slots[index];
                using var prepared = ScaleForOcr(slot);
                using var pix = PixConverter.ToPix(prepared);
                using var page = engine.Process(pix, PageSegMode.SingleBlock);
                var rawText = CleanOcrText(page.GetText());
                rewards.Add(MatchReward(index, rawText, catalog));
            }

            var matched = rewards.Count(reward => reward.ItemId is not null);
            var candidate = new ScanResult(
                matched == 0 ? "no_rewards" : "ok",
                matched == 0
                    ? "Награды не распознаны. Откройте экран выбора награды или используйте ручной запуск."
                    : matched < rewards.Count ? "Часть наград распознана неуверенно." : null,
                screenshot.Width,
                screenshot.Height,
                theme.Name,
                rewards);
            best = ChooseBetterResult(best, candidate);
            if (matched == slotCount)
            {
                break;
            }
        }
        return best ?? ScanResult.Error("no_rewards", "Награды не распознаны.");
    }

    private static ScanResult ChooseBetterResult(ScanResult? current, ScanResult candidate)
    {
        if (current is null || current.Status == "scan_retry")
        {
            return candidate;
        }

        if (candidate.Rewards.Count != current.Rewards.Count)
        {
            return candidate.Rewards.Count > current.Rewards.Count ? candidate : current;
        }

        var mergedRewards = current.Rewards
            .Zip(candidate.Rewards, (previous, next) =>
            {
                if (previous.ItemId is null && next.ItemId is not null) return next;
                if (previous.ItemId is not null && next.ItemId is null) return previous;
                return next.Confidence > previous.Confidence ? next : previous;
            })
            .ToArray();
        var mergedMatched = mergedRewards.Count(reward => reward.ItemId is not null);
        var mergedComplete = mergedMatched >= 2 && mergedMatched == mergedRewards.Length;
        if (mergedMatched > 0)
        {
            return new ScanResult(
                "ok",
                mergedComplete ? null : "Часть наград распознана неуверенно.",
                candidate.CaptureWidth ?? current.CaptureWidth,
                candidate.CaptureHeight ?? current.CaptureHeight,
                candidate.Theme ?? current.Theme,
                mergedRewards);
        }

        return candidate.Rewards.Sum(reward => reward.Confidence)
            > current.Rewards.Sum(reward => reward.Confidence)
            ? candidate
            : current;
    }

    private static int CountMatched(ScanResult result) =>
        result.Rewards.Count(reward => reward.ItemId is not null);

    internal static void RunVisualFallback(
        int parentProcessId,
        WatchRequest request,
        RewardWatcherState watcherState)
    {
        try
        {
            var tessdata = ResolveTessdata(null);
            using var engine = CreateRussianEngine(tessdata);
            while (IsProcessRunning(parentProcessId))
            {
                Thread.Sleep(1_500);
                if (!NativeMethods.IsWarframeForeground()) continue;
                try
                {
                    using var screenshot = CaptureWarframeWindow();
                    var uiScale = ResolveUiScale(request.UiScale);
                    if (!LooksLikeRewardScreen(screenshot, uiScale)) continue;
                    var catalog = watcherState.BuildCatalog(request);
                    var candidates = BuildCandidates(catalog);
                    if (candidates.Count == 0) continue;
                    var result = ScanFrame(screenshot, uiScale, engine, candidates);
                    if (CountMatched(result) < 2) continue;
                    if (watcherState.TryEmitReward("visual"))
                    {
                        Thread.Sleep(16_000);
                    }
                }
                catch (Exception)
                {
                    // Warframe can change resolution, minimize, or close between the checks.
                    // The next lightweight poll retries without stopping the DBWIN watcher.
                }
            }
        }
        catch (Exception error)
        {
            watcherState.Emit(new { type = "visual_error", message = error.Message });
        }
    }

    private static IReadOnlyList<CatalogCandidate> BuildCandidates(IEnumerable<CatalogItem> catalog) =>
        catalog
            .Where(item => !string.IsNullOrWhiteSpace(item.Name))
            .Select(item => new CatalogCandidate(item, Normalize(item.Name)))
            .ToArray();

    private static bool LooksLikeRewardScreen(Bitmap screenshot, double uiScale)
    {
        var scale = Math.Min(
            screenshot.Width / (double)ReferenceWidth,
            screenshot.Height / (double)ReferenceHeight);
        if (scale <= 0.2) return false;
        var theme = DetectTheme(screenshot, scale);
        using var strip = CropRewardStrip(screenshot, scale, uiScale);
        var populatedSections = CountPopulatedRewardSections(strip, theme);
        return populatedSections >= 2;
    }

    private static int CountPopulatedRewardSections(Bitmap strip, ThemeInfo theme)
    {
        var populatedSections = 0;
        var totalTextPixels = 0;
        for (var section = 0; section < 4; section++)
        {
            var left = section * strip.Width / 4;
            var right = (section + 1) * strip.Width / 4;
            var sectionPixels = 0;
            for (var y = 0; y < strip.Height; y++)
            {
                for (var x = left; x < right; x++)
                {
                    if (IsThemeText(strip.GetPixel(x, y), theme)) sectionPixels++;
                }
            }
            totalTextPixels += sectionPixels;
            if (sectionPixels >= 18) populatedSections++;
        }
        return totalTextPixels >= 60 ? populatedSections : 0;
    }

    private static bool IsProcessRunning(int processId)
    {
        try
        {
            using var process = Process.GetProcessById(processId);
            return !process.HasExited;
        }
        catch (ArgumentException)
        {
            return false;
        }
    }

    private static Bitmap LoadScreenshot(string path)
    {
        if (!File.Exists(path)) throw new FileNotFoundException("Файл снимка не найден.", path);
        using var source = new Bitmap(path);
        return new Bitmap(source);
    }

    private static double ResolveUiScale(double? requestedScale)
    {
        if (requestedScale is >= 0.5 and <= 1.25)
        {
            return requestedScale.Value;
        }

        try
        {
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            var configPath = Path.Combine(localAppData, "Warframe", "EE.cfg");
            if (!File.Exists(configPath))
            {
                return 1.0;
            }

            const string prefix = "Flash.FlashDrawScale=";
            foreach (var line in File.ReadLines(configPath))
            {
                if (!line.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                var value = line[prefix.Length..].Trim();
                if (!double.TryParse(
                        value,
                        System.Globalization.NumberStyles.Float,
                        System.Globalization.CultureInfo.InvariantCulture,
                        out var rawScale))
                {
                    return 1.0;
                }

                var roundedPercent = (int)Math.Round(rawScale * 20.0) * 5;
                return Math.Clamp(roundedPercent / 100.0, 0.5, 1.25);
            }
        }
        catch (IOException)
        {
            return 1.0;
        }
        catch (UnauthorizedAccessException)
        {
            return 1.0;
        }

        return 1.0;
    }

    private static Bitmap CaptureWarframeWindow()
    {
        var bounds = FindWarframeWindowBounds();
        var bitmap = new Bitmap(bounds.Width, bounds.Height, PixelFormat.Format32bppArgb);
        using var graphics = Graphics.FromImage(bitmap);
        graphics.CopyFromScreen(bounds.Left, bounds.Top, 0, 0, bounds.Size, CopyPixelOperation.SourceCopy);
        return bitmap;
    }

    private static Rectangle FindWarframeWindowBounds()
    {
        var process = Process.GetProcesses()
            .Where(item => item.ProcessName.Contains("Warframe", StringComparison.OrdinalIgnoreCase))
            .OrderByDescending(item => item.MainWindowHandle != IntPtr.Zero)
            .FirstOrDefault(item => item.MainWindowHandle != IntPtr.Zero)
            ?? throw new InvalidOperationException("Окно Warframe не найдено.");

        if (!NativeMethods.TryGetClientBounds(process.MainWindowHandle, out var bounds)
            || bounds.Width < 640
            || bounds.Height < 360)
        {
            throw new InvalidOperationException("Не удалось определить область окна Warframe.");
        }
        return bounds;
    }

    private static string ResolveTessdata(string? _configuredPath)
    {
        return ExtractEmbeddedTessdata();
    }

    private static TesseractEngine CreateRussianEngine(string tessdata)
    {
        var engine = new TesseractEngine(tessdata, OcrLanguage, EngineMode.Default);
        engine.SetVariable("tessedit_char_whitelist", RussianCharacterWhitelist);
        return engine;
    }

    private static bool IsUsableTessdata(string path)
    {
        try
        {
            return new FileInfo(path).Length > 1_000_000;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }

    private static string ExtractEmbeddedTessdata()
    {
        var cacheDirectory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "PlatScope",
            "ocr",
            "tessdata-rus-v1");
        Directory.CreateDirectory(cacheDirectory);
        var destination = Path.Combine(cacheDirectory, "rus.traineddata");
        using var embedded = typeof(Program).Assembly.GetManifestResourceStream(EmbeddedTessdataName)
            ?? throw new InvalidDataException("В OCR-модуле отсутствует встроенная языковая модель.");
        if (IsUsableTessdata(destination) && new FileInfo(destination).Length == embedded.Length)
        {
            return cacheDirectory;
        }

        var temporary = destination + $".tmp-{Environment.ProcessId}";
        try
        {
            using (var output = new FileStream(temporary, FileMode.Create, FileAccess.Write, FileShare.None))
            {
                embedded.CopyTo(output);
                output.Flush(true);
            }
            File.Move(temporary, destination, true);
        }
        finally
        {
            if (File.Exists(temporary))
            {
                File.Delete(temporary);
            }
        }
        return cacheDirectory;
    }

    private static Bitmap CropRewardStrip(Bitmap screenshot, double screenScale, double uiScale)
    {
        var effectiveScale = screenScale * uiScale;
        var cropWidth = Math.Clamp((int)Math.Round(RewardWidth * effectiveScale), 4, screenshot.Width);
        var left = Math.Max(0, screenshot.Width / 2 - cropWidth / 2);
        var top = screenshot.Height / 2
            - (int)Math.Round((RewardYDisplay - RewardHeight + RewardLineHeight) * effectiveScale);
        var bottom = screenshot.Height / 2
            - (int)Math.Round((RewardYDisplay - RewardHeight) * effectiveScale);
        top = Math.Clamp(top, 0, screenshot.Height - 1);
        bottom = Math.Clamp(bottom, top + 1, screenshot.Height);
        return screenshot.Clone(
            new Rectangle(left, top, Math.Min(cropWidth, screenshot.Width - left), bottom - top),
            PixelFormat.Format32bppArgb);
    }

    private static List<Bitmap> SplitAndFilter(Bitmap strip, ThemeInfo theme, int slotCount)
    {
        using var filtered = new Bitmap(strip.Width, strip.Height, PixelFormat.Format32bppArgb);
        for (var y = 0; y < strip.Height; y++)
        {
            for (var x = 0; x < strip.Width; x++)
            {
                filtered.SetPixel(x, y, IsThemeText(strip.GetPixel(x, y), theme) ? Color.Black : Color.White);
            }
        }

        var referenceSlotWidth = Math.Max(1, filtered.Width / 4);
        var activeWidth = Math.Min(filtered.Width, referenceSlotWidth * slotCount);
        var activeLeft = Math.Max(0, (filtered.Width - activeWidth) / 2);
        var slotWidth = Math.Max(1, activeWidth / slotCount);
        var slots = new List<Bitmap>(slotCount);
        for (var index = 0; index < slotCount; index++)
        {
            var left = activeLeft + index * slotWidth;
            var width = index == slotCount - 1 ? activeLeft + activeWidth - left : slotWidth;
            slots.Add(filtered.Clone(
                new Rectangle(left, 0, width, filtered.Height), PixelFormat.Format32bppArgb));
        }
        return slots;
    }

    private static Bitmap ScaleForOcr(Bitmap source)
    {
        if (source.Height >= 100) return new Bitmap(source);
        var ratio = 100d / source.Height;
        var output = new Bitmap((int)Math.Ceiling(source.Width * ratio), 100, PixelFormat.Format32bppArgb);
        using var graphics = Graphics.FromImage(output);
        graphics.InterpolationMode = InterpolationMode.HighQualityBicubic;
        graphics.PixelOffsetMode = PixelOffsetMode.HighQuality;
        graphics.DrawImage(source, 0, 0, output.Width, output.Height);
        return output;
    }

    private static RewardMatch MatchReward(int slot, string rawText, IReadOnlyList<CatalogCandidate> catalog)
    {
        var normalized = Normalize(rawText);
        if (normalized.Length < 4) return new RewardMatch(slot, rawText, null, null, null, 0);

        CatalogCandidate? best = null;
        var bestDistance = int.MaxValue;
        foreach (var candidate in catalog)
        {
            var distance = Levenshtein(normalized, candidate.NormalizedName);
            if (distance < bestDistance)
            {
                best = candidate;
                bestDistance = distance;
            }
        }

        if (best is null) return new RewardMatch(slot, rawText, null, null, null, 0);
        var length = Math.Max(normalized.Length, best.NormalizedName.Length);
        var confidence = length == 0 ? 0 : Math.Clamp(1d - bestDistance / (double)length, 0, 1);
        var allowedDistance = Math.Max(3, (int)Math.Ceiling(normalized.Length * 0.45));
        if (bestDistance > allowedDistance || confidence < 0.55)
        {
            return new RewardMatch(slot, rawText, null, null, null, confidence);
        }

        return new RewardMatch(slot, rawText, best.Source.ItemId, best.Source.Slug, best.Source.Name, confidence);
    }

    private static string CleanOcrText(string value) => string.Join(
        ' ', value.Replace('\r', ' ').Replace('\n', ' ')
            .Split(' ', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries));

    private static string Normalize(string value)
    {
        var upper = value.ToUpperInvariant()
            .Replace("BLUEPRINT", string.Empty, StringComparison.Ordinal)
            .Replace("ЧЕРТЁЖ", string.Empty, StringComparison.Ordinal)
            .Replace("ЧЕРТЕЖ", string.Empty, StringComparison.Ordinal)
            .Replace('Ё', 'Е');
        var builder = new StringBuilder(upper.Length);
        foreach (var character in upper)
        {
            if (!char.IsLetterOrDigit(character)) continue;
            builder.Append(character switch
            {
                'A' => 'А',
                'B' => 'В',
                'C' => 'С',
                'E' => 'Е',
                'H' => 'Н',
                'K' => 'К',
                'M' => 'М',
                'O' => 'О',
                'P' => 'Р',
                'T' => 'Т',
                'X' => 'Х',
                'Y' => 'У',
                _ => character,
            });
        }
        return builder.ToString();
    }

    private static int Levenshtein(string left, string right)
    {
        if (left.Length == 0) return right.Length;
        if (right.Length == 0) return left.Length;
        var previous = Enumerable.Range(0, right.Length + 1).ToArray();
        var current = new int[right.Length + 1];
        for (var leftIndex = 1; leftIndex <= left.Length; leftIndex++)
        {
            current[0] = leftIndex;
            for (var rightIndex = 1; rightIndex <= right.Length; rightIndex++)
            {
                var substitution = previous[rightIndex - 1]
                    + (left[leftIndex - 1] == right[rightIndex - 1] ? 0 : 1);
                current[rightIndex] = Math.Min(
                    Math.Min(current[rightIndex - 1] + 1, previous[rightIndex] + 1), substitution);
            }
            (previous, current) = (current, previous);
        }
        return previous[right.Length];
    }

    private static ThemeInfo DetectTheme(Bitmap screenshot, double scale)
    {
        var probeX = Math.Clamp((int)Math.Round(150 * scale), 0, screenshot.Width - 1);
        var probeTop = Math.Clamp((int)Math.Round(85 * scale), 0, screenshot.Height - 1);
        var probeBottom = Math.Clamp((int)Math.Round(93 * scale), probeTop, screenshot.Height - 1);
        var midpoint = (probeTop + probeBottom) / 2;
        var top = AverageVerticalPixels(screenshot, probeX, probeTop, midpoint);
        var bottom = AverageVerticalPixels(screenshot, probeX, midpoint + 1, probeBottom);
        return Themes.All
            .OrderBy(theme => ColorDistance(top, theme.ProbeTop) + ColorDistance(bottom, theme.ProbeBottom))
            .First();
    }

    private static Color AverageVerticalPixels(Bitmap image, int x, int fromY, int throughY)
    {
        if (fromY > throughY) return Color.Black;
        long red = 0, green = 0, blue = 0;
        var count = 0;
        for (var y = fromY; y <= throughY; y++)
        {
            var color = image.GetPixel(x, y);
            red += color.R;
            green += color.G;
            blue += color.B;
            count++;
        }
        return count == 0
            ? Color.Black
            : Color.FromArgb((int)(red / count), (int)(green / count), (int)(blue / count));
    }

    private static double ColorDistance(Color left, Color right)
    {
        var red = left.R - right.R;
        var green = left.G - right.G;
        var blue = left.B - right.B;
        return Math.Sqrt(red * red + green * green + blue * blue);
    }

    private static bool IsThemeText(Color color, ThemeInfo theme)
    {
        var hue = color.GetHue();
        var saturation = color.GetSaturation();
        var brightness = color.GetBrightness();
        var primaryDistance = HueDistance(hue, theme.Primary.GetHue());
        var secondaryDistance = HueDistance(hue, theme.Secondary.GetHue());
        return theme.Name switch
        {
            "Lotus" => primaryDistance < 5 && saturation >= 0.65 && Math.Abs(brightness - theme.Primary.GetBrightness()) <= 0.1
                || secondaryDistance < 15 && brightness >= 0.65,
            "Orokin" => primaryDistance < 5 && brightness <= 0.42 && saturation >= 0.1
                || secondaryDistance < 5 && brightness is >= 0.25f and <= 0.5f && saturation >= 0.25,
            "Equinox" => saturation <= 0.2 && brightness >= 0.55,
            "Deadlock" => saturation <= 0.08 && brightness >= 0.80,
            "Lunar Renewal" => saturation <= 0.15 && brightness >= 0.85,
            "Legacy" => brightness >= 0.65 || secondaryDistance < 6 && brightness >= 0.5 && saturation >= 0.5,
            "Tenno" => (primaryDistance < 3 || secondaryDistance < 2) && saturation >= 0.38 && brightness <= 0.55,
            "High Contrast" => (primaryDistance < 3 || secondaryDistance < 2) && saturation >= 0.49 && brightness >= 0.35,
            "Corpus" => primaryDistance < 3 && brightness >= 0.42 && saturation >= 0.35,
            "Grineer" => primaryDistance < 5 && brightness > 0.5 || secondaryDistance < 6 && brightness > 0.55,
            "Conquera" => primaryDistance < 25 && saturation >= 0.20 && brightness is >= 0.15f and <= 0.65f
                || saturation <= 0.25 && brightness >= 0.55,
            _ => (primaryDistance < 6 || secondaryDistance < 8) && saturation >= 0.2 && brightness >= 0.2,
        };
    }

    private static double HueDistance(double left, double right)
    {
        var distance = Math.Abs(left - right);
        return Math.Min(distance, 360 - distance);
    }
}

internal sealed record ScanRequest(
    List<CatalogItem> Catalog,
    string? ImagePath,
    string? TessdataPath,
    double? UiScale,
    int? MaxAttempts,
    int? RetryIntervalMs,
    int? InitialDelayMs);
internal sealed record WatchRequest(
    List<CatalogItem> Catalog,
    List<WatchRelic> Relics,
    double? UiScale);
internal sealed record WatchRelic(string RelicGameRef, List<string> RewardSlugs);
internal sealed record CatalogItem(string ItemId, string Slug, string Name);
internal sealed record CatalogCandidate(CatalogItem Source, string NormalizedName);
internal sealed record RewardMatch(int Slot, string RawText, string? ItemId, string? Slug, string? Name, double Confidence);

internal sealed record ScanResult(
    string Status,
    string? Message,
    int? CaptureWidth,
    int? CaptureHeight,
    string? Theme,
    IReadOnlyList<RewardMatch> Rewards)
{
    internal static ScanResult Error(string status, string message) =>
        new(status, message, null, null, null, Array.Empty<RewardMatch>());
}

internal sealed record ThemeInfo(string Name, Color Primary, Color Secondary, Color ProbeTop, Color ProbeBottom);

internal sealed class RewardWatcherState
{
    private static readonly TimeSpan ProjectionGroupGap = TimeSpan.FromSeconds(30);
    private static readonly TimeSpan RewardEventCooldown = TimeSpan.FromSeconds(60);
    private readonly object gate = new();
    private readonly HashSet<string> activeRelicPaths = new(StringComparer.Ordinal);
    private DateTime lastProjectionAt = DateTime.MinValue;
    private DateTime lastRewardAt = DateTime.MinValue;

    internal bool AddProjection(string path)
    {
        lock (gate)
        {
            var now = DateTime.UtcNow;
            var reset = now - lastProjectionAt > ProjectionGroupGap;
            if (reset) activeRelicPaths.Clear();
            activeRelicPaths.Add(path);
            lastProjectionAt = now;
            return reset;
        }
    }

    internal IReadOnlyList<CatalogItem> BuildCatalog(WatchRequest request)
    {
        lock (gate)
        {
            if (activeRelicPaths.Count == 0) return request.Catalog;
            var allowed = request.Relics
                .Where(relic => activeRelicPaths.Contains(relic.RelicGameRef))
                .SelectMany(relic => relic.RewardSlugs)
                .ToHashSet(StringComparer.Ordinal);
            if (allowed.Count == 0) return request.Catalog;
            // The projection log is only a hint. A player who joined later may not have emitted a
            // projection path visible to us, so excluding the rest of the catalog loses rewards.
            return request.Catalog
                .OrderBy(item => item.Slug != "forma_blueprint" && !allowed.Contains(item.Slug))
                .ToArray();
        }
    }

    internal void ClearProjections()
    {
        lock (gate)
        {
            activeRelicPaths.Clear();
            lastProjectionAt = DateTime.MinValue;
            lastRewardAt = DateTime.MinValue;
        }
    }

    internal bool TryEmitReward(string source)
    {
        lock (gate)
        {
            var now = DateTime.UtcNow;
            if (now - lastRewardAt < RewardEventCooldown) return false;
            lastRewardAt = now;
            EmitLocked(new { type = "reward", source });
            return true;
        }
    }

    internal void Emit(object value)
    {
        lock (gate)
        {
            EmitLocked(value);
        }
    }

    private static void EmitLocked(object value)
    {
        Console.Out.WriteLine(JsonSerializer.Serialize(value));
        Console.Out.Flush();
    }
}

internal static class Themes
{
    internal static IReadOnlyList<ThemeInfo> All { get; } = new[]
    {
        Theme("Vitruvian", 190,169,102, 245,227,173, 189,168,101, 26,22,24),
        Theme("Stalker", 153,31,35, 255,61,51, 152,31,35, 17,4,4),
        Theme("Baruuk", 238,193,105, 236,211,162, 237,192,104, 60,55,43),
        Theme("Corpus", 35,201,245, 111,229,253, 35,200,244, 7,39,63),
        Theme("Fortuna", 57,105,192, 255,115,230, 57,105,191, 7,9,34),
        Theme("Grineer", 255,189,102, 255,224,153, 254,188,101, 18,27,16),
        Theme("Lotus", 36,184,242, 255,241,191, 36,183,241, 39,53,96),
        Theme("Nidus", 140,38,92, 245,73,93, 139,38,91, 220,211,197),
        Theme("Orokin", 20,41,29, 178,125,5, 20,41,29, 203,209,208),
        Theme("Tenno", 9,78,106, 6,106,74, 9,78,105, 183,204,207),
        Theme("High Contrast", 102,176,255, 255,255,0, 101,175,254, 15,31,61),
        Theme("Legacy", 255,255,255, 232,213,93, 254,254,254, 35,60,70),
        Theme("Equinox", 158,159,167, 232,227,227, 157,159,166, 19,12,21),
        Theme("Dark Lotus", 140,119,147, 200,169,237, 139,119,146, 41,11,85),
        Theme("Zephyr", 253,132,2, 255,53,0, 252,132,2, 27,26,27),
        Theme("Conquera", 200,100,200, 255,215,0, 254,254,254, 177,66,182),
        Theme("Deadlock", 25,35,60, 255,255,255, 254,254,254, 30,40,62),
        Theme("Lunar Renewal", 160,40,40, 255,200,100, 254,254,254, 101,28,29),
        Theme("POM 2", 105,185,140, 100,255,100, 129,223,150, 11,47,31),
    };

    private static ThemeInfo Theme(
        string name,
        int pr, int pg, int pb,
        int sr, int sg, int sb,
        int tr, int tg, int tb,
        int br, int bg, int bb) =>
        new(name, Color.FromArgb(pr, pg, pb), Color.FromArgb(sr, sg, sb),
            Color.FromArgb(tr, tg, tb), Color.FromArgb(br, bg, bb));
}

internal static class NativeMethods
{
    private const int GwlStyle = -16;
    private const long WsPopup = 0x80000000L;

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetWindowRect(IntPtr handle, out NativeRect rect);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetClientRect(IntPtr handle, out NativeRect rect);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ClientToScreen(IntPtr handle, ref NativePoint point);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", SetLastError = true)]
    private static extern IntPtr GetWindowLongPtr(IntPtr handle, int index);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetProcessDpiAwarenessContext(IntPtr value);

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    private static extern void OutputDebugStringW(string outputString);

    internal static void TryEnablePerMonitorDpiAwareness() => _ = SetProcessDpiAwarenessContext(new IntPtr(-4));

    internal static void EmitDebugLine(string line) => OutputDebugStringW(line);

    internal static bool IsWarframeForeground()
    {
        var handle = GetForegroundWindow();
        if (handle == IntPtr.Zero) return false;
        _ = GetWindowThreadProcessId(handle, out var processId);
        if (processId == 0) return false;
        try
        {
            using var process = Process.GetProcessById((int)processId);
            return process.ProcessName.Contains("Warframe", StringComparison.OrdinalIgnoreCase);
        }
        catch (ArgumentException)
        {
            return false;
        }
        catch (InvalidOperationException)
        {
            return false;
        }
    }

    internal static bool TryGetClientBounds(IntPtr handle, out Rectangle rectangle)
    {
        rectangle = Rectangle.Empty;
        if (!GetWindowRect(handle, out var window)) return false;
        var style = GetWindowLongPtr(handle, GwlStyle).ToInt64();
        if ((style & WsPopup) != 0)
        {
            rectangle = Rectangle.FromLTRB(window.Left, window.Top, window.Right, window.Bottom);
            return true;
        }

        if (!GetClientRect(handle, out var client)) return false;
        var origin = new NativePoint();
        if (!ClientToScreen(handle, ref origin)) return false;
        rectangle = new Rectangle(origin.X, origin.Y, client.Right - client.Left, client.Bottom - client.Top);
        return true;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeRect
    {
        internal int Left;
        internal int Top;
        internal int Right;
        internal int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativePoint
    {
        internal int X;
        internal int Y;
    }
}

internal static class DbwinRewardWatcher
{
    private const uint PageReadWrite = 0x04;
    private const uint FileMapRead = 0x0004;
    private const uint WaitObject0 = 0;
    private const int ErrorAlreadyExists = 183;
    private const int BufferSize = 4096;
    private const uint WaitTimeoutMs = 500;

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateFileMappingW(
        IntPtr file,
        IntPtr attributes,
        uint protect,
        uint maximumSizeHigh,
        uint maximumSizeLow,
        string name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr MapViewOfFile(
        IntPtr mapping,
        uint desiredAccess,
        uint fileOffsetHigh,
        uint fileOffsetLow,
        UIntPtr bytesToMap);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool UnmapViewOfFile(IntPtr address);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateEventW(
        IntPtr attributes,
        [MarshalAs(UnmanagedType.Bool)] bool manualReset,
        [MarshalAs(UnmanagedType.Bool)] bool initialState,
        string name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetEvent(IntPtr handle);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(IntPtr handle);

    internal static int Run(int parentProcessId, bool allowAnyProcess, WatchRequest? watcherRequest)
    {
        if (parentProcessId <= 0)
        {
            Console.Error.WriteLine("DBWIN parent process id is missing.");
            return 1;
        }

        Thread.CurrentThread.Priority = ThreadPriority.Highest;
        var mapping = CreateFileMappingW(
            new IntPtr(-1),
            IntPtr.Zero,
            PageReadWrite,
            0,
            BufferSize,
            "DBWIN_BUFFER");
        if (mapping == IntPtr.Zero)
        {
            Console.Error.WriteLine($"CreateFileMappingW failed: {Marshal.GetLastWin32Error()}");
            return 1;
        }

        var alreadyExists = Marshal.GetLastWin32Error() == ErrorAlreadyExists;
        var view = MapViewOfFile(mapping, FileMapRead, 0, 0, UIntPtr.Zero);
        var ready = CreateEventW(IntPtr.Zero, false, true, "DBWIN_BUFFER_READY");
        var data = CreateEventW(IntPtr.Zero, false, false, "DBWIN_DATA_READY");
        if (view == IntPtr.Zero || ready == IntPtr.Zero || data == IntPtr.Zero)
        {
            if (view != IntPtr.Zero) _ = UnmapViewOfFile(view);
            if (ready != IntPtr.Zero) _ = CloseHandle(ready);
            if (data != IntPtr.Zero) _ = CloseHandle(data);
            _ = CloseHandle(mapping);
            Console.Error.WriteLine($"DBWIN initialization failed: {Marshal.GetLastWin32Error()}");
            return 1;
        }

        var watcherState = new RewardWatcherState();
        Thread? visualThread = null;
        if (watcherRequest is { Catalog.Count: > 0 })
        {
            visualThread = new Thread(() => Program.RunVisualFallback(
                parentProcessId,
                watcherRequest,
                watcherState))
            {
                IsBackground = true,
                Name = "PlatScope reward visual fallback",
                Priority = ThreadPriority.BelowNormal,
            };
            visualThread.Start();
        }

        var pidCache = new Dictionary<int, bool>();
        var bytes = new byte[BufferSize - sizeof(int)];
        watcherState.Emit(new
        {
            type = "ready",
            alreadyExists,
            visualFallback = visualThread is not null,
        });

        try
        {
            while (ParentIsRunning(parentProcessId))
            {
                if (WaitForSingleObject(data, WaitTimeoutMs) != WaitObject0)
                {
                    continue;
                }

                var sourceProcessId = Marshal.ReadInt32(view);
                Marshal.Copy(IntPtr.Add(view, sizeof(int)), bytes, 0, bytes.Length);
                _ = SetEvent(ready);
                if (!allowAnyProcess && !IsWarframeProcess(sourceProcessId, pidCache))
                {
                    continue;
                }

                var length = Array.IndexOf(bytes, (byte)0);
                if (length <= 0) continue;
                var line = Encoding.UTF8.GetString(bytes, 0, length);
                if (TryGetProjectionPath(line, out var projectionPath))
                {
                    var reset = watcherState.AddProjection(projectionPath);
                    watcherState.Emit(new
                    {
                        type = "projection",
                        path = projectionPath,
                        reset,
                    });
                }
                if (line.Contains(
                        "ProjectionRewardChoice.lua: Relic reward screen shut down",
                        StringComparison.Ordinal))
                {
                    watcherState.ClearProjections();
                    watcherState.Emit(new { type = "projection_clear" });
                }
                if (IsRewardMarker(line))
                {
                    _ = watcherState.TryEmitReward("dbwin");
                }
            }
        }
        finally
        {
            _ = UnmapViewOfFile(view);
            _ = CloseHandle(mapping);
            _ = CloseHandle(ready);
            _ = CloseHandle(data);
        }

        return 0;
    }

    private static bool IsRewardMarker(string line) =>
        line.Contains("ProjectionRewardChoice.lua: Got rewards", StringComparison.Ordinal)
        || line.Contains("ProjectionRewardChoice.lua: Missing icon data!", StringComparison.Ordinal);

    private static bool TryGetProjectionPath(string line, out string path)
    {
        const string prefix = "/Lotus/Types/Game/Projections/";
        path = string.Empty;
        var start = line.IndexOf(prefix, StringComparison.Ordinal);
        if (start < 0) return false;
        var end = start;
        while (end < line.Length && line[end] is not ')' and not ' ' and not '\r' and not '\n')
        {
            end++;
        }
        if (end <= start + prefix.Length) return false;
        path = line[start..end];
        return !path.EndsWith(".png", StringComparison.OrdinalIgnoreCase)
            && !path.EndsWith(".lua", StringComparison.OrdinalIgnoreCase);
    }

    private static bool ParentIsRunning(int parentProcessId)
    {
        try
        {
            using var process = Process.GetProcessById(parentProcessId);
            return !process.HasExited;
        }
        catch (ArgumentException)
        {
            return false;
        }
    }

    private static bool IsWarframeProcess(int processId, Dictionary<int, bool> cache)
    {
        if (cache.TryGetValue(processId, out var cached)) return cached;
        if (cache.Count >= 256) cache.Clear();
        try
        {
            using var process = Process.GetProcessById(processId);
            var result = process.ProcessName.Contains("Warframe", StringComparison.OrdinalIgnoreCase);
            cache[processId] = result;
            return result;
        }
        catch (ArgumentException)
        {
            return false;
        }
        catch (InvalidOperationException)
        {
            return false;
        }
    }
}
