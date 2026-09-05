using Tesseract;

namespace PlatScope.RewardOcr;

internal static partial class Program
{
    // Выполняется только по --self-test-russian: без игры, захвата экрана и сети.
    private static bool RunMatchingRegression(TesseractEngine engine)
    {
        var items = new[]
        {
            new CatalogItem("bronco", "bronco_prime_blueprint", "Бронко Прайм (Чертеж)"),
            new CatalogItem("akbronco", "akbronco_prime_blueprint", "Акбронко Прайм (Чертеж)"),
            new CatalogItem("lex", "lex_prime_blueprint", "Лекс Прайм (Чертеж)"),
            new CatalogItem("aklex", "aklex_prime_blueprint", "Аклекс Прайм (Чертеж)"),
            new CatalogItem("vasto", "vasto_prime_blueprint", "Васто Прайм (Чертеж)"),
            new CatalogItem("akvasto", "akvasto_prime_blueprint", "Аквасто Прайм (Чертеж)"),
            new CatalogItem("magnus", "magnus_prime_blueprint", "Магнус Прайм (Чертеж)"),
            new CatalogItem("akmagnus", "akmagnus_prime_blueprint", "Акмагнус Прайм (Чертеж)"),
            new CatalogItem("braton", "braton_prime_barrel", "Брэйтон Прайм: Ствол"),
            new CatalogItem("paris", "paris_prime_blueprint", "Парис Прайм (Чертеж)"),
            new CatalogItem("forma", "forma_blueprint", "Чертёж: Форма"),
            new CatalogItem("forma", "forma_blueprint", "Форма (Чертеж)"),
            new CatalogItem("lohk", "lohk", "Лок"),
        };
        var valid = true;
        void Check(bool condition, string message)
        {
            if (condition) return;
            valid = false;
            Console.Error.WriteLine($"Ошибка проверки OCR: {message}");
        }

        // Подсказки из журнала меняют порядок кандидатов, но не сам предмет.
        foreach (var ordered in new[] { items, items.Reverse().ToArray() })
        {
            var catalog = BuildCandidates(ordered);
            foreach (var item in items)
            {
                Check(MatchReward(0, item.Name, catalog).ItemId == item.ItemId, item.Name);
            }
            Check(MatchReward(0, "Чертёж: Акбронко\nПрайм", catalog).ItemId == "akbronco",
                "двухстрочный чертёж Акбронко");
            Check(MatchReward(0, "Брэитон Прайм: Ствол", catalog).ItemId == "braton",
                "обычная ошибка одной буквы");
        }
        foreach (var item in items.Where(item => item.ItemId.StartsWith("ak", StringComparison.Ordinal)))
        {
            var withoutPairedWeapon = BuildCandidates(items.Where(candidate => candidate.ItemId != item.ItemId));
            Check(MatchReward(0, item.Name, withoutPairedWeapon).ItemId is null,
                $"отсутствующая награда не заменяется одиночной: {item.Name}");
            var singleId = item.ItemId[2..];
            var single = items.Single(candidate => candidate.ItemId == singleId);
            var withoutSingleWeapon = BuildCandidates(items.Where(candidate => candidate.ItemId != singleId));
            Check(MatchReward(0, single.Name, withoutSingleWeapon).ItemId is null,
                $"отсутствующая награда не заменяется парной: {single.Name}");
        }
        var ambiguous = BuildCandidates(new[]
        {
            new CatalogItem("mag", "mag_prime_blueprint", "Маг Прайм (Чертеж)"),
            new CatalogItem("lag", "lag_prime_blueprint", "Лаг Прайм (Чертеж)"),
        });
        Check(MatchReward(0, "Чертёж: Баг Прайм", ambiguous).ItemId is null,
            "равнозначные кандидаты не выбираются по порядку");
        Check(MatchReward(0, "Чертёж: Баг Прайм", ambiguous.Reverse().ToArray()).ItemId is null,
            "неоднозначность не зависит от порядка");
        var labels = new[]
        {
            "Брэйтон Прайм: Ствол", "Чертёж: Акбронко\nПрайм", "Чертёж: Форма", "Чертёж: Парис Прайм",
        };
        var expected = new[] { "braton", "akbronco", "forma", "paris" };
        using var screenshot = BuildRussianSelfTestScreenshot(labels);
        var result = ScanFrame(screenshot, 1.0, engine, BuildCandidates(items));
        Check(result.Rewards.Select(reward => reward.ItemId).SequenceEqual(expected),
            "экран с Акбронко и похожим Бронко в каталоге");
        return valid;
    }
}
