using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.RegularExpressions;
using Tesseract;

namespace PlatScope.RewardOcr;

internal sealed record RelicPreparationReading(string? Era, string? RelicName, string? Refinement);

// Экран подготовки не подтверждает расход реликвии. Здесь читаем только видимый
// фильтр эры и заголовок собственной выделенной реликвии, без карточек других копий.
internal sealed class RelicPreparationOcr : IDisposable
{
    private readonly TesseractEngine engine = Program.CreateRelicPreparationEngine();
    private readonly Action<string>? trace;

    internal RelicPreparationOcr(Action<string>? trace = null) => this.trace = trace;

    internal RelicPreparationReading? Scan(Bitmap screenshot)
    {
        if (screenshot.Width < 640 || screenshot.Height < 360) return null;

        var header = Read(screenshot, .081, .022, .323, .082, PageSegMode.SingleLine);
        trace?.Invoke($"header {header.Confidence:F3}: {header.Text}");
        var compactHeader = Compact(header.Text);
        if (header.Confidence < .30f || !compactHeader.Contains("РЕЛИКВИИ", StringComparison.Ordinal)
            || !compactHeader.Contains("БЕЗДНЫ", StringComparison.Ordinal))
            return null;

        var eraLine = Read(screenshot, .034, .091, .174, .127, PageSegMode.SingleLine);
        trace?.Invoke($"era {eraLine.Confidence:F3}: {eraLine.Text}");
        var screenEra = eraLine.Confidence >= .35f ? ParseEraHeading(eraLine.Text) : null;

        // Останавливаемся до бокового оверлея. Полное название его содержимого
        // не должно попасть в OCR собственной выбранной реликвии.
        var title = Read(screenshot, .624, .243, .779, .282, PageSegMode.SingleLine);
        trace?.Invoke($"title {title.Confidence:F3}: {title.Text}");
        var own = title.Confidence >= .35f ? ParseOwnTitle(title.Text) : null;
        if (screenEra is not null && own is not null && screenEra != own.Era) return null;

        return new RelicPreparationReading(screenEra ?? own?.Era, own?.RelicName, own?.Refinement);
    }

    private (string Text, float Confidence) Read(Bitmap screenshot, double left, double top,
        double right, double bottom, PageSegMode mode)
    {
        using var prepared = Prepare(screenshot, left, top, right, bottom);
        using var pix = PixConverter.ToPix(prepared);
        using var page = engine.Process(pix, mode);
        return (Normalize(page.GetText()), page.GetMeanConfidence());
    }

    private static Bitmap Prepare(Bitmap screenshot, double left, double top, double right, double bottom)
    {
        var crop = Rectangle.FromLTRB((int)Math.Floor(screenshot.Width * left),
            (int)Math.Floor(screenshot.Height * top), (int)Math.Ceiling(screenshot.Width * right),
            (int)Math.Ceiling(screenshot.Height * bottom));
        crop.Intersect(new Rectangle(0, 0, screenshot.Width, screenshot.Height));
        using var binary = new Bitmap(crop.Width, crop.Height, PixelFormat.Format32bppArgb);
        using (var graphics = Graphics.FromImage(binary))
            graphics.DrawImage(screenshot, new Rectangle(0, 0, crop.Width, crop.Height), crop, GraphicsUnit.Pixel);

        var data = binary.LockBits(new Rectangle(0, 0, binary.Width, binary.Height),
            ImageLockMode.ReadWrite, PixelFormat.Format32bppArgb);
        try
        {
            var pixels = new byte[Math.Abs(data.Stride) * binary.Height];
            Marshal.Copy(data.Scan0, pixels, 0, pixels.Length);
            for (var y = 0; y < binary.Height; y++)
            for (var x = 0; x < binary.Width; x++)
            {
                var offset = y * data.Stride + x * 4;
                var bright = (pixels[offset] + pixels[offset + 1] + pixels[offset + 2]) / 3 >= 160;
                var value = bright ? (byte)0 : (byte)255;
                pixels[offset] = pixels[offset + 1] = pixels[offset + 2] = value;
                pixels[offset + 3] = 255;
            }
            Marshal.Copy(pixels, 0, data.Scan0, pixels.Length);
        }
        finally { binary.UnlockBits(data); }

        const int enlargement = 3;
        const int padding = 12;
        var result = new Bitmap(binary.Width * enlargement + padding * 2,
            binary.Height * enlargement + padding * 2, PixelFormat.Format32bppArgb);
        using (var graphics = Graphics.FromImage(result))
        {
            graphics.Clear(Color.White);
            graphics.InterpolationMode = InterpolationMode.NearestNeighbor;
            graphics.PixelOffsetMode = PixelOffsetMode.Half;
            graphics.DrawImage(binary, new Rectangle(padding, padding,
                binary.Width * enlargement, binary.Height * enlargement));
        }
        return result;
    }

    private static string? ParseEraHeading(string text)
    {
        var match = Regex.Match(text, @"\b(ЛИТ|LITH|МЕЗО|MESO|НЕО|NEO|АКСИ|AXI|РЕКВИЕМ|REQUIEM)\s+ЭРА\b",
            RegexOptions.CultureInvariant);
        return match.Success ? CanonicalEra(match.Groups[1].Value) : null;
    }

    private static RelicPreparationReading? ParseOwnTitle(string text)
    {
        var match = Regex.Match(text,
            @"\b(?:РЕЛИКВИЯ\s+)?(ЛИТ|LITH|МЕЗО|MESO|НЕО|NEO|АКСИ|AXI|РЕКВИЕМ|REQUIEM)\s+([A-ZА-ЯЁ][0-9]{1,3}|IV|III|II|I)\b",
            RegexOptions.CultureInvariant);
        if (!match.Success) return null;
        var era = CanonicalEra(match.Groups[1].Value);
        var code = NormalizeCode(match.Groups[2].Value);
        if (era is null || code is null || (era == "requiem" && !Regex.IsMatch(code, @"^(I|II|III|IV)$"))
            || (era != "requiem" && !Regex.IsMatch(code, @"^[A-Z][1-9][0-9]{0,2}$")))
            return null;
        var name = era switch { "lith" => "Lith", "meso" => "Meso", "neo" => "Neo",
            "axi" => "Axi", "requiem" => "Requiem", _ => null };
        if (name is null) return null;
        string? refinement = null;
        foreach (var (word, value) in new[] { ("НЕТРОНУТАЯ", "intact"), ("ИСКЛЮЧИТЕЛЬНАЯ", "exceptional"),
                     ("БЕЗУПРЕЧНАЯ", "flawless"), ("СИЯЮЩАЯ", "radiant"), ("INTACT", "intact"),
                     ("EXCEPTIONAL", "exceptional"), ("FLAWLESS", "flawless"), ("RADIANT", "radiant") })
        {
            if (!Regex.IsMatch(text, $@"\b{word}\b", RegexOptions.CultureInvariant)) continue;
            if (refinement is not null && refinement != value) return null;
            refinement = value;
        }
        return new RelicPreparationReading(era, $"{name} {code}", refinement);
    }

    private static string? CanonicalEra(string text) => text switch
    {
        "ЛИТ" or "LITH" => "lith", "МЕЗО" or "MESO" => "meso", "НЕО" or "NEO" => "neo",
        "АКСИ" or "AXI" => "axi", "РЕКВИЕМ" or "REQUIEM" => "requiem", _ => null,
    };

    private static string? NormalizeCode(string text)
    {
        var code = string.Concat(text.Select(character => character switch
        {
            'А' => 'A', 'В' => 'B', 'Е' => 'E', 'К' => 'K', 'М' => 'M', 'Н' => 'H', 'О' => 'O',
            'Р' => 'P', 'С' => 'C', 'Т' => 'T', 'Х' => 'X', 'У' => 'Y', _ => character,
        }));
        return code.All(character => character is >= 'A' and <= 'Z' or >= '0' and <= '9') ? code : null;
    }

    private static string Normalize(string text) => Regex.Replace(text.ToUpperInvariant().Replace('Ё', 'Е'), @"\s+", " ").Trim();
    private static string Compact(string text) => Regex.Replace(text, @"[^А-ЯA-Z0-9]", "");
    public void Dispose() => engine.Dispose();
}

internal static partial class Program
{
    internal static TesseractEngine CreateRelicPreparationEngine() => CreateRussianEngine(ResolveTessdata(null));

    private static int RunRelicPreparationImageScan(string path)
    {
        using var screenshot = LoadScreenshot(path);
        using var reader = new RelicPreparationOcr(text => Console.Error.WriteLine(text));
        var reading = reader.Scan(screenshot);
        Console.Out.Write(JsonSerializer.Serialize(reading is null
            ? new { status = "not_visible", reading = (RelicPreparationReading?)null }
            : new { status = "ok", reading = (RelicPreparationReading?)reading }, JsonOptions));
        return reading is null ? 2 : 0;
    }
}
