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
            new CatalogItem("ash", "ash_prime_chassis_blueprint", "Эш Прайм: Каркас (Чертеж)"),
            new CatalogItem("revenant", "revenant_prime_chassis_blueprint", "Ревенант Прайм: Каркас (Чертеж)"),
            new CatalogItem("banshee", "banshee_prime_chassis_blueprint", "Банши Прайм: Каркас (Чертеж)"),
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
            Check(MatchReward(0, "Чертёж: Ревенант\nПрайм: Каркас", catalog).ItemId == "revenant",
                "Ревенант с двухстрочным названием не заменяется Эшем");
            Check(MatchReward(0, "Чертёж: Ревенамт Праим: Каркас", catalog).ItemId == "revenant",
                "ошибки OCR в имени и слове Прайм не мешают Ревенанту");
            foreach (var partial in new[] { "Прайм: Каркас", "Чертёж: Прайм: Каркас", "Праим: Каркас", "Прайн: Каркас", "Прайк: Каркас", "Ж Прайм: Каркас" })
            {
                Check(MatchReward(0, partial, catalog).ItemId is null,
                    $"обрывок без имени не превращается в Эша: {partial}");
            }
        }
        Check(MatchReward(0, "Чертёж: Ревенант Прайм: Каркас",
            BuildCandidates(items.Where(item => item.ItemId != "revenant"))).ItemId is null,
            "отсутствующий Ревенант не подменяется другим варфреймом по слову Каркас");
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
        var chassisLabels = new[] { "Чертёж: Парис Прайм", "Чертёж: Ревенант\nПрайм: Каркас", "Брэйтон Прайм: Ствол", "Чертёж: Банши\nПрайм: Каркас" };
        using var completeFrame = BuildRussianSelfTestScreenshot(chassisLabels);
        var complete = ScanFrame(completeFrame, 1.0, engine, BuildCandidates(items));
        Check(complete.Rewards.Select(reward => reward.ItemId).SequenceEqual(new[] { "paris", "revenant", "braton", "banshee" }),
            "экран с двумя каркасами сохраняет точные имена варфреймов");
        chassisLabels[1] = "Прайм: Каркас";
        using var partialFrame = BuildRussianSelfTestScreenshot(chassisLabels);
        var partialResult = ScanFrame(partialFrame, 1.0, engine, BuildCandidates(items));
        Check(partialResult.Rewards.Count == 4 && partialResult.Rewards[1].ItemId is null,
            "кадр с непрочитанной первой строкой не угадывает каркас Эша");
        var recovered = ChooseBetterResult(partialResult, complete);
        Check(recovered.Rewards.Count == 4 && recovered.Rewards[1].ItemId == "revenant",
            "следующий полный кадр восстанавливает Ревенанта");
        return valid;
    }
}
