# Уведомления о стороннем коде

## Warframe: игровой значок освоения

`apps/desktop/src/lib/assets/warframe-mastered.png` — оригинальная текстура
Warframe `/Lotus/Interface/Icons/CodexObjectsLaurelWhite.png`, © Digital Extremes.
Используется для обозначения освоенного снаряжения; права на игровую графику
принадлежат Digital Extremes, а не проекту PlatScope.

Соответствие игровому маркеру `MASTERED` проверено по
[ExportTextIcons.json](https://github.com/calamity-inc/warframe-public-export-plus/blob/senpai/ExportTextIcons.json).
Файл получен без перерисовки из [зеркала игровых ресурсов browse.wf](https://browse.wf/Lotus/Interface/Icons/CodexObjectsLaurelWhite.png).
SHA-256: `239249f3ffb73e3c94ce8b5cda9b2d8c3ff6ba82832a30ac187462b795ad9f06`.
Цвет в интерфейсе задаётся маской, исходная форма и прозрачность сохранены.

## WFInfo

Геометрия экрана наград, палитра тем и устройство OCR-пайплайна в
`apps/reward-ocr` адаптированы из проекта
[WFInfo](https://github.com/WFCD/WFinfo), лицензированного по Apache License 2.0.

Copyright WFInfo contributors.

## WFHelper

Управление повторными кадрами OCR, DBWIN-триггер, защита от дублей и выбор
лучшего результата адаптированы из проекта
[WFHelper](https://github.com/WFHelper/WFHelper), лицензированного по MIT License.

MIT License

Copyright (c) 2026 WFHelper

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Tesseract OCR

Помощник наград использует Tesseract OCR и русские обучающие данные `rus`,
лицензированные по Apache License 2.0:
https://github.com/tesseract-ocr/tesseract
https://github.com/tesseract-ocr/tessdata_fast/tree/4.1.0

Встроенный файл `rus.traineddata`: SHA-256
`E16E5E036CCE1D9EC2B00063CF8B54472625B9E14D893A169E2B0DEDEB4DF225`.

## TennoWorth

Read-only scanner в `crates/platscope-readonly-scan` адаптирован из проекта
[TennoWorth](https://github.com/tennoworth/tennoworth), commit
`1b77d0b830f019bae22fb15bbff28cde606aa7b2`.

State machine торговых диалогов `EE.log`, фильтрация служебных строк и удаление
platform glyphs в `apps/desktop/src-tauri/src/trade_log.rs` адаптированы из того
же проекта, commit `0e0d12d0c2efade26976561e5f6f341c91673955`.

MIT License

Copyright (c) 2026

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
