# Пакет заявки PlatScope Companion для Overwolf

Статус: готовый draft, **не отправлен**. Для подачи требуется Overwolf developer account и решения владельца продукта, перечисленные ниже.

## Почему это отдельный публичный продукт

Overwolf относит приложения, существующие только как невидимый мост к другому сервису, к private apps и сейчас их не одобряет. [Project Roadmap](https://dev.overwolf.com/ow-native/getting-started/project-roadmap/) требует публичное приложение как минимум с одним desktop window, public-facing features и monetization plan.

Поэтому заявляемый продукт — `PlatScope Companion`: видимое локальное desktop-приложение для контроля снимка инвентаря Warframe. Интеграция с PlatScope является одной из функций, но окно полезно само по себе:

- показывает, подключён ли Warframe GEP;
- показывает доступность `match_info.inventory` и понятную причину degraded state;
- показывает время последнего coherent snapshot и количество распознанных строк;
- показывает агрегаты по категориям без ценовых обещаний;
- позволяет вручную повторить синхронизацию;
- показывает точный локальный путь экспорта и открывает каталог;
- позволяет включить/выключить локальный export;
- объясняет privacy и отсутствие server upload.

Persistent in-game overlay, подсказки во время боя и market orders не входят в scope.

## Короткое описание на русском

PlatScope Companion — локальная панель состояния инвентаря Warframe. Она использует поддерживаемое событие Overwolf GEP `match_info.inventory`, показывает свежесть и полноту снимка и по явному согласию сохраняет versioned JSON на компьютере пользователя. Данные не отправляются на сервер. Экспорт можно использовать в PlatScope или другом локальном инструменте, поддерживающем открытую схему.

## Submission-ready English answers

### App name

PlatScope Companion

### One-line summary

A visible, privacy-first Warframe inventory snapshot dashboard with an optional local JSON export for PlatScope.

### App idea and player value

PlatScope Companion gives Warframe players a clear view of whether supported inventory data is available, when the latest coherent snapshot was observed, and whether local export is healthy. Its desktop window shows connection and GEP health, snapshot freshness, recognized item counts, category summaries, and recovery instructions when events are unavailable. Players can explicitly enable a local versioned JSON export and choose its destination. The app remains useful as a standalone data-health and inventory-summary dashboard; PlatScope integration is optional.

### Public-facing features

1. A desktop status window that clearly indicates the app is running.
2. Warframe/GEP availability and degraded-state explanations.
3. Last coherent inventory snapshot time, item-row count, and non-market category summaries.
4. Explicit controls for local export, destination, and manual retry.
5. Privacy panel listing exactly which fields are written locally.
6. Optional “Open in PlatScope” action when PlatScope is installed.

### Game and supported feature

- Game: Warframe, Overwolf game ID `8954`.
- Required GEP feature: `match_info`.
- Consumed info update: `inventory` only.
- `username`, `highlighted`, and `chat` are not collected.

The official [Warframe GEP page](https://dev.overwolf.com/ow-native/live-game-data-gep/supported-games/warframe) documents `match_info.inventory` and warns that feature availability can change. The UI therefore exposes GEP health and never labels stale data as current.

### Framework

Implemented: Overwolf Native WebApp for the smallest permission and runtime surface. Source package and validated unpacked build находятся в `apps/overwolf-companion`; DevRel feedback после approval может потребовать совместимых manifest/UX-изменений, но расширение permissions без отдельного review запрещено.

### Data flow

```text
Warframe
  -> Overwolf GEP match_info.inventory
  -> in-memory validation
  -> visible local summary
  -> optional versioned JSON file chosen by the player
```

There is no PlatScope cloud service. Inventory data is not uploaded, sold, used for advertising targeting, or written to analytics. No account ID, nonce, chat, username, WFM credential, or process-memory fragment is collected.

### Permissions

- `GameInfo`: obtain the supported Warframe game information/event stream.
- `FileSystem`: write the user-enabled local export through documented [`overwolf.io.writeFileContents`](https://dev.overwolf.com/ow-native/reference/io/ow-io/#writefilecontentsfilepath-content-encoding-triggeruacifrequired-callback).

No `GameControl`, capture, microphone, webcam, hotkey, or native plugin permission is requested in the initial scope.

### Game compliance

The app does not alter Warframe, inject code, read process memory, automate inputs, place market orders, or display decision-making overlays during active gameplay. It consumes only an Overwolf-supported info update and follows the [Overwolf game compliance rules](https://dev.overwolf.com/ow-electron/guides/game-compliance/overview/). All market analysis remains outside the companion and is based on user-opened PlatScope views.

### User experience

The always-identifiable desktop window contains:

```text
PlatScope Companion
Warframe: running / not running
Inventory event: available / degraded / unavailable
Last snapshot: absolute timestamp
Recognized rows: N
Local export: on/off
Destination: <user-visible path>
[Sync now] [Open export folder]
Privacy: local only; no server upload
```

Errors remain visible until resolved and say what the player can do. No overlay appears during active gameplay. First-run onboarding asks for export consent before any file is created.

### Monetization

**OWNER DECISION REQUIRED.** Current Overwolf onboarding states that an approved public app must integrate Overwolf ads or subscriptions. Recommended proposal option: an optional Overwolf supporter subscription that does not gate inventory capture, export, privacy controls, or recovery. Possible supporter benefits must remain cosmetic/non-essential, for example additional dashboard themes.

Advertisements are not recommended for this small utility because they add distraction and a broader privacy surface. Do not submit this answer until the owner accepts a monetization model and DevRel confirms that the proposed subscription is sufficient.

### Support and legal

**OWNER INPUT REQUIRED:** public support email or page, Terms of Use URL, Privacy Policy URL, publisher identity, and deletion/contact procedure. Release review requires public legal URLs that do not require sign-in.

## Privacy disclosure draft

PlatScope Companion processes the Warframe inventory event locally on your device. If you enable export, it writes a versioned JSON file to the path you choose. The file contains the observation time, Warframe/feature identifiers, and inventory item identifiers and quantities. The app does not upload inventory data to PlatScope or another server. It does not collect your Warframe username, chat, account ID, nonce, Warframe.Market credentials, or process memory. Disabling export stops future file writes; you can delete the existing file at any time.

Это текст продукта, а не юридическая консультация. Перед публикацией он должен стать публичной Privacy Policy с реквизитами издателя и применимой юрисдикцией.

## Review checklist

### До подачи идеи

- [ ] владелец подтверждает публичный companion-dashboard, а не private bridge;
- [ ] выбран Overwolf account/publisher identity;
- [ ] выбрана модель subscriptions или ads;
- [ ] готовы support/contact URL;
- [ ] проверены актуальные Warframe EULA и Overwolf terms;
- [ ] English answers перенесены в app idea form без расширения permissions.

### После approval, до разработки OPK

- [ ] DevRel письменно подтверждает `match_info.inventory` для заявленного use case;
- [x] зафиксированы Native WebApp и minimum Overwolf version `0.170.0`;
- [ ] manifest содержит только согласованные permissions;
- [ ] privacy/legal URLs опубликованы;
- [x] создан отдельный package `apps/overwolf-companion` с независимым versioning;
- [ ] GEP fixtures очищены от персональных данных;
- [x] реализованы GEP health, visible desktop window и opt-in export automation;

### До публикации

- [ ] QA на текущем Warframe build и временно недоступном GEP;
- [ ] отсутствие server inventory upload подтверждено network inspection;
- [ ] invalid/partial payload не заменяет предыдущий coherent export;
- [ ] uninstall/deletion behavior документирован;
- [ ] accessibility, localization, privacy и store-listing review завершены;
- [ ] build загружен в Developer Console только из утверждённого source tree.

## Что требуется от владельца для подачи

1. Войти или зарегистрироваться в [Overwolf app idea form](https://dev.overwolf.com/app-idea-form).
2. Подтвердить, что companion будет публичным самостоятельным приложением с видимым окном.
3. Выбрать publisher name и monetization plan.
4. Предоставить support, Privacy Policy и Terms URLs.
5. Отправить English draft и сохранить ответ/номер заявки в документации проекта.

Codex не отправляет заявку без явного поручения: подача создаёт внешнее обязательство от имени владельца и требует его аккаунта и продуктовых решений.
