# Проверки первой реализации

Дата: 2026-11-08. Среда: Windows, PowerShell, Python 3.14.3.

## Редактор нод, параметров и связей

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1,
eframe/egui 0.33.3, JDK 21.0.13:

- `cargo xtask ci --offline` — PASS: fmt/check/test/Clippy, слои,
  18 command contracts и 9 NodeSpec. 81 Rust-тест, одна subprocess fixture ignored
  (выполняется отдельным процессом из теста отмены).
- Новые проверки desktop — PASS: typed round-trip всех нод; ошибки u32/f64,
  NaN/Inf, Boolean, строки/списки; input events инспектора с Apply/Revert и Undo;
  сохранение буфера при смене selection/New/Open; отказ Save при ошибке числа
  без создания файла; палитра, Delete/Disconnect и Undo с incident edges;
  save/load неполного графа; output→input через pointer events, Escape;
  отказ при цикле/несовместимом типе без изменения графа/истории.
- Новые проверки engine — PASS: project.new защищает dirty/active job,
  проверяет имя, сбрасывает path/history/checkpoint, сохраняет пустой граф.
  New/Open после завершения job не возвращают старые результаты при polling.
  Для проверки lifecycle используется явно synthetic scheduler, не fake Spark.
- Отдельный opt-in `run_button_executes_real_spark_and_preview_is_rendered`
  с feature spark-integration-tests — PASS: Run через egui input и настоящий
  Spark вернул (4,11)/(5,13)/(6,15). Затем UI изменил Filter на quantity>=5,
  Apply и повторный Run вернули точно (5,13)/(6,15)/(7,17). New очистил результат.
  Это один дополнительный тест вне обычных 81, с двумя настоящими JVM jobs.
- Clippy desktop с feature spark-integration-tests — PASS.
- `cargo xtask fixtures --offline --java <JDK21/bin/java.exe>` — PASS:
  sample/draft/regression/prediction совпали с JSON oracle по схемам и значениям.
- Native startup sample с --smoke-frames 3 — PASS, exit 0, project loaded=true.
  Просмотрен новый снимок настоящего Glow window: палитра, New project, граф
  с пользовательской раскладкой. `.tools/graph-editor-window.png` ignored.
- Повторный `job_lifecycle` на настоящем JVM — PASS: cancellation confirmed
  after cleanup, следующий запуск успешен, stale revision 1/current 2 обнаружен.
- Scala worker не менялся, sbt tests повторно — NOT_RUN; их прежний PASS
  остаётся историческим. GitHub CI новой итерации, Linux/macOS GUI и независимое
  review новых UI/lifecycle изменений — NOT_RUN.

Сохранённый пользователем `examples/pipelines/sample.dcraft` содержит изменённые
координаты нод; файл не откатывался. Два старых Session-теста сначала упали из-за
предположения об исходной позиции (0,0). Их helper теперь задаёт собственную
детерминированную входную раскладку, независимо от редактируемого примера.
Тестовые assertions атомарности и Undo сохранены. Тест модального перехода
учитывает начальный sizing pass egui Area перед проверкой/нажатием кнопок.

Пользователь до этой итерации подтвердил native preview, движение нод и отсутствие
наблюдаемых зависаний; это приёмка предыдущего viewer, не всех новых жестов.
Ручная приёмка полного редактора/Save as/dirty-close/Cancel ещё нужна.
Логи: `.tools/editor-ci.log`, `editor-tests.log`, `editor-spark.log`,
`editor-fixtures.log`, `editor-native-final.log`, `editor-clippy-spark.log`,
`editor-lifecycle.log`. Дешёвый validator и git diff --check — PASS.
ASSUMPTION: ML threshold 1e-6 не изменён, его solver error bound не обоснован.

## Первый desktop-клиент

verified 2026-11-08 via локальные команды агента, Windows/MSVC, Rust 1.99.1,
eframe/egui 0.33.3, JDK 21.0.13:

- Первоначальный fetch в ограниченной сессии — FAIL: нет доступа к index.crates.io.
  Пользователь затем выполнил fetch в PowerShell: 264 crates загружены.
  Последующие Cargo checks выполнялись по реальному Cargo.lock из кэша.
- `cargo xtask ci --offline` — PASS: fmt/check/test/Clippy, layers, commands,
  nodes. 70 Rust-тестов (66 прежних + 4 desktop); subprocess fixture по-прежнему
  единственная ignored-функция. Desktop в layer=ui; headless crates без egui.
- Desktop tests — PASS: неподвижность Project при render/selection,
  сохранение проекта/selection/view при ошибке Open, сохранение точки при zoom,
  реальные pointer events drag с ровно одной Move action после release.
  Проверяется также, что строка Project path рисуется внутри toolbar clip rect.
- `cargo test --locked --offline -p datacraft-desktop --features
  spark-integration-tests run_button_executes_real_spark_and_preview_is_rendered`
  с DATACRAFT_JAVA=JDK21/bin/java.exe — PASS: отдельный opt-in тест нажимает
  Run через egui input, дожидается настоящего Spark и проверяет точные строки
  (4,11)/(5,13)/(6,15), одну таблицу и отрисованный preview с числом строк.
  Его нет в обычных 70 тестах, он запускался отдельно, без fake worker response.
- Дополнительный Clippy desktop с feature spark-integration-tests — PASS.
- Native startup `cargo run --locked --offline -p datacraft-desktop --
  examples/pipelines/sample.dcraft --smoke-frames 3` — PASS, exit 0:
  отрисованы нативные frames, project loaded=true, приложение завершилось.
- Снимок настоящего Glow window через диагностический feature eframe/__screenshot
  — PASS: видны пять нод, связи, путь проекта, кнопки, Inspector и Results.
  После просмотра исправлены тема и обрезание второй строки toolbar; добавлен
  regression assertion. Снимок: `.tools/desktop-window.png` (локальный, ignored).
- Scala-код не менялся; sbt tests и четыре fixture pipeline повторно не
  запускались в этой итерации. Свежий полный PASS baseline — предыдущий блок.
  Новый Spark UI-тест, в отличие от этих исторических проверок, исполнен сейчас.
- GitHub workflow дополнен Linux native smoke через Xvfb, системными GUI
  prerequisites и отдельным Spark UI-тестом. Этот обновлённый workflow — NOT_RUN
  удалённо; ранее прошедший Linux headless CI относится к ffb6288.

Логи: `.tools/desktop-ci-final.log`, `desktop-tests.log`, `desktop-spark-test.log`,
`desktop-native.log`, `desktop-screenshot.log`, `desktop-clippy-spark.log`.
Ручные native сценарии Save as/dirty-close/Cancel/Undo и независимое review
нового UI — NOT_RUN. Headless UI input test не заменяет эту ручную приёмку.
Создание/удаление нод, редактирование параметров/соединений, native file picker
не реализованы; параметры только для чтения. ASSUMPTION: прежний ML threshold
1e-6 не изменён и по-прежнему не является доказанной границей ошибки solver.

## Полный прогон xtask в пользовательском PowerShell

verified 2026-11-08 via предоставленный пользователем полный вывод
`cargo fetch --locked` и `cargo xtask ci --with-spark`, Windows/JDK 21.0.13,
завершение 14:29 по местному времени. Это отдельный пользовательский запуск,
не повторное выполнение в ограниченной сессии агента.

- fetch — PASS: недостающие 18 Cargo packages загружены.
- fmt/check/test/Clippy — PASS: 66 Rust-тестов; одна ignored-функция служит
  subprocess fixture и запускается прошедшим тестом отмены процесса.
- layers — PASS: полный разрешённый normal/dev/build/target dependency graph.
- commands/nodes — PASS: 17 команд и 9 определений нод.
- sbt test writeClasspath — PASS: 6 Scala-тестов с настоящим Spark 4.0.1.
- fixtures — PASS: sample/draft/regression/prediction, сравнение таблиц,
  схем и значений с независимым oracle.
- job_lifecycle — PASS: отмена после cleanup JVM, успешный следующий job,
  обнаружение stale результата (snapshot revision 1, current revision 2).
- Итог: `[PASS] requested CI checks completed`.

Ограничения Cargo/sbt-кэша предыдущего агентского запуска не воспроизвелись
в пользовательском терминале. Удалённый GitHub CI и Linux/macOS этим запуском
не проверены. ASSUMPTION: ML-допуск 1e-6 остаётся предварительным acceptance
threshold, не доказанной границей ошибки Spark solver.

## Итерация xtask — предыдущий запуск в ограниченной сессии

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1, JDK 21.0.13:

- fmt/check/test/Clippy — PASS. Workspace: 66 пройденных тестов, включая
  9 новых тестов политики зависимостей и comparator/manifest. Одна ignored-функция
  остаётся subprocess fixture, запускаемой тестом отмены настоящего процесса.
- `cargo xtask commands --offline` — PASS: 17 команд и независимый тест
  соответствия enum/serde/parameter contracts.
- `cargo xtask nodes --offline` — PASS: 9 нод, порты, параметры, defaults,
  typed factories и семь registry contract tests.
- `cargo xtask fixtures --offline --java <JDK21/bin/java.exe>` — PASS:
  sample/draft/regression/prediction выполнены настоящим Spark через Session.
  Схемы, имена таблиц, число строк и значения совпали с отдельным oracle
  `examples/expected/pipelines.json`; draft имеет две ожидаемые readiness issues.
- `cargo xtask layers --offline` — FAIL до анализа: Cargo metadata для всех
  платформ требует отсутствующий в локальном кэше bitflags 2.13.2.
  Повторный запуск без offline также FAIL: static.crates.io недоступен
  (в диагностике, среди прочего, slab 0.4.12). Политика намеренно не заменена
  урезанной проверкой только текущей ОС. Её шесть unit tests прошли, но это
  не подтверждение успешного анализа реального полного workspace graph.
- `cargo xtask ci --offline` — FAIL на layers после успешных Rust checks.
  Полный `ci --with-spark` — NOT_RUN. Для повторения в обычном терминале:
  `cargo fetch --locked`, затем `cargo xtask ci --with-spark`.
- Повторный `sbt test writeClasspath` — FAIL при запуске launcher: нет доступа
  к пользовательскому sbt boot/cache. Scala-тесты в этой итерации — NOT_RUN;
  предыдущие шесть PASS сохраняются как исторический результат. Fixtures
  использовали уже собранный worker и существующий classpath.txt.
- GitHub workflow обновлён: Rust job запускает xtask ci; Spark job после
  sbt проверяет fixtures и сохраняет CLI inspect/lifecycle smoke checks.
  Новый удалённый workflow, Linux/macOS и независимое review — NOT_RUN.

Логи локальные, игнорируются Git: `.tools/xtask-ci.log`, `xtask-layers.log`,
`xtask-fixtures.log`, `xtask-sbt.log`. Layers не использует grep по Cargo.toml:
нужен разрешённый Cargo metadata graph с normal/dev/build/target dependencies.
ASSUMPTION: прежний абсолютный ML-допуск 1e-6 остаётся provisional acceptance
threshold; численная эквивалентность solver не доказана. Fixtures явно печатает
это допущение и не расширяет допуск по результату запуска.

## Итерация фоновых jobs и отмены

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1, JDK 21.0.13:

- cargo fmt --all -- --check — PASS.
- cargo check --workspace --locked --offline — PASS.
- cargo test --workspace --locked --offline — PASS: 57 тестов
  (22 core, 8 project, 23 engine, 4 CLI). Одна дополнительная ignored-функция —
  subprocess fixture с sleep; она запускается отдельным настоящим процессом
  из cancel_terminates_real_child_and_unblocks_reader. Это не пропущенная
  проверка отмены: сам тест завершения процесса и разблокировки reader прошёл.
- cargo clippy --workspace --all-targets --locked --offline -- -D warnings — PASS.
- job_lifecycle example на настоящем JVM — exit 0: отмена после регистрации
  worker, подтверждённый cleanup, следующий successful job, изменение проекта
  во время работы и обнаружение stale результата revision 1 при current 2.
  После финальной правки этапа Executing пример повторно прошёл.
- CLI --timeout-seconds 1 run sample.dcraft — ожидаемый exit 1; диагностирован
  именно TimedOut, а не ошибка аргументов. Runner дождался очистки JVM.
- CLI sample/draft/regression/prediction и inspect sales.csv --rows 3 — exit 0
  через новую фоновую реализацию. Sample/draft: (4,11)/(5,13)/(6,15);
  regression: intercept≈3, coefficient≈2, training RMSE 3.13e-15;
  Predict: 5/7/9/11/13 с округлением; inspect: три строки и исходная схема.
- Дешёвая статика и git diff --check — PASS.

Логи: .tools/jobs-checks/*.stdout.log, *.stderr.log и lifecycle-final.log.
Unit tests контролируют scheduling через каналы/Condvar; synthetic results
используются только для state machine, не как доказательство работы Spark.
Отдельно проверены stop до установки handle, first-stop-wins, Busy, неизвестный
Job ID, terminal events, bounded queue, timeout без polling, panic containment,
best-effort stop при Drop и редактирование Session во время job.

Run теперь возвращает JobStarted; cancel/status зарегистрированы и исполняемы.
Вычисления и supervisor работают в отдельных std threads. Terminal status
публикуется после cleanup. Каждый job создаёт JVM; graceful Spark group cancel,
проценты выполнения и Ctrl+C CLI не реализованы. Новых зависимостей нет.

Scala/protocol не менялись; sbt tests повторно не запускались. Новые CI-шаги,
Linux/macOS и независимое review новой реализации — NOT_RUN. Во время проверки
Clippy потребовал перенести Drop impl перед test module; замечание исправлено.
ASSUMPTION: прежний ML-допуск 1e-6 по-прежнему требует численного обоснования.

## Итерация Session и команд

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1, JDK 21.0.13:

- cargo fmt --all -- --check — PASS.
- cargo check --workspace --locked --offline — PASS.
- cargo test --workspace --locked --offline — PASS: 49 тестов
  (22 core, 8 project, 15 engine, 4 CLI).
- cargo clippy --workspace --all-targets --locked --offline -- -D warnings — PASS.
- CLI commands --json: exit 0; 15 зарегистрированных исполняемых команд,
  без фиктивной execution.cancel. Отдельный CLI-тест использует отсутствующую Java.
- CLI validate sample и validate draft --draft: exit 0; черновик сообщает
  две проблемы незавершённой ноды.
- CLI run draft/sample/regression/prediction и inspect sales.csv --rows 3:
  все exit 0 через Session и настоящий Spark worker. Draft/sample вернули
  (4,11)/(5,13)/(6,15); regression — коэффициенты около 3/2 и RMSE 3.13e-15;
  Predict — ожидаемые 5/7/9/11/13; inspect — три строки и исходную схему.
  Логи: .tools/session-checks/*.stdout.log и *.stderr.log.
- Статическая проверка артефактов и git diff --check — PASS.

Проверены rollback ошибочных команд/Batch, восстановление рёбер/output,
монотонные revisions, no-op/redo, лимит истории, dirty-checkpoints,
ошибки open/save и rebasing current/undo/redo при SaveAs.
Во время разработки исправлены конфликт имени Command со std::process::Command,
неустойчивый порядок рёбер после reconnect и имя CLI-варианта, отклонённое Clippy.
После исправлений команды выше выполнены успешно.

CLI теперь использует Session для open/validate/run/inspect. История содержит
только Project. Runtime config, JVM и ExecutionRecord отделены от .dcraft;
результат помечается revision. Синхронный dispatch Run ещё нельзя отменить
конкурентно; background jobs, progress, timeout и GUI в эту итерацию не входят.

Для тестов engine добавлен tempfile как dev-dependency; пакет уже был в lock,
новые пакеты не загружались. Scala/protocol не менялись, sbt tests не повторялись.
Новые шаги CI не запускались удалённо; новая версия Linux/macOS и независимое
review — NOT_RUN. ASSUMPTION: прежний ML-допуск 1e-6 остаётся необоснованным;
эквивалентность исходным нодам не заявляется.

## Итерация черновых проектов

verified 2026-11-08 via локальные команды Windows, Rust 1.99.1, JDK 21.0.13:

- fmt --check, cargo check --workspace --locked --offline — PASS.
- cargo test --workspace --locked --offline — PASS: 33 теста
  (9 graph, 6 core drafts, 7 registry, 8 project, 3 CLI).
- cargo clippy --workspace --all-targets --locked --offline -- -D warnings — PASS.
- CLI validate draft.dcraft --draft — exit 0, две ожидаемые диагностики
  expression/missing table input на ноде UUID ...0006.
- CLI run draft.dcraft — exit 0, строки (4,11)/(5,13)/(6,15), schema integer.
  Неподключённая нода ...0006 не включена в план исполнения.
- Прежние sample/regression/prediction выполнены через настоящий Spark worker,
  все exit 0; регрессия сохранила коэффициенты около 3/2 и RMSE 3.13e-15.
- Проверки malformed save сохраняют прежние байты файла; NaN/Inf не записываются
  как null. Неполная выбранная ветка отклоняется до попытки запустить Java.
- Дешёвая статическая проверка и git diff --check — PASS.

Логи CLI: .tools/draft-checks/*.stdout.log и *.stderr.log. В первом тестовом
прогоне обнаружена неоднозначность типа пустого Vec в assert_eq; тест исправлен
на is_empty, затем все проверки выше выполнены успешно.
Для CLI-тестов добавлена dev-dependency tempfile, уже используемая project crate;
Cargo.lock обновлён настоящим Cargo offline, новые пакеты не загружались.

Scala worker не менялся, sbt tests повторно не запускались. Linux/macOS и
удалённый CI новой итерации — NOT_RUN. Приёмка независимым reviewer — NOT_RUN.
ASSUMPTION: прежний численный допуск ML-тестов 1e-6 ещё требует обоснования;
эта итерация меняет только правила допуска черновиков и выбора ветки.

## Итерация NodeSpec

verified 2026-11-08 via локальные команды Windows/MSVC, Rust 1.99.1:

- cargo fmt --all -- --check — PASS.
- cargo check --workspace --locked --offline — PASS.
- cargo test --workspace --locked --offline — PASS: 21 тест
  (9 graph, 7 registry, 4 project, 1 CLI discovery).
- cargo clippy --workspace --all-targets --locked --offline -- -D warnings — PASS.
- Прежние sample/regression/prediction .dcraft выполнены через настоящий
  Rust CLI → Spark worker, все exit 0. Sample дал (4,11)/(5,13)/(6,15);
  regression — intercept 2.9999999999999987, coefficient 2.0000000000000004,
  training RMSE 3.1297001304428877e-15; Predict — 5/7/9/11/13 с округлением.
  Логи: .tools/nodespec-checks/{sample,regression,prediction}.{stdout,stderr}.log.
- CLI nodes --json проверен интеграционным тестом с заведомо отсутствующей
  Java: discovery работает без JVM. В тестах проверены старые поля JSON,
  типы/порты всех девяти нод, defaults, границы и ошибки metadata.
- Дешёвая статическая проверка артефактов — PASS; git diff --check — PASS.

Scala worker и протокол не менялись. sbt test в этой итерации не повторялся:
его предыдущий результат 6/6 сохраняется как историческое свидетельство,
а интеграция после изменения core проверена тремя настоящими CLI-прогонами.
Новый шаг CI nodes --json добавлен, но изменения ещё не отправлены на GitHub:
удалённый CI для этой итерации NOT_RUN. Независимое review и macOS/Linux
прогон новой версии также NOT_RUN. Обоснование прежнего ML-допуска 1e-6
остаётся открытым ASSUMPTION и этим рефакторингом не решается.

## Актуальный baseline после установки инструментов

verified 2026-11-08 via реальные локальные команды: Rust/Cargo 1.99.1,
Windows x86_64 MSVC, Temurin JDK 21.0.13, Spark 4.0.1.

| Проверка | Результат |
|---|---|
| cargo fmt --all -- --check | PASS после применения cargo fmt --all |
| cargo check --workspace --locked --offline | PASS |
| cargo test --workspace --locked --offline | PASS, 9 graph + 4 project tests |
| cargo clippy --workspace --all-targets --locked --offline -- -D warnings | PASS |
| CLI run regression.dcraft | PASS, exit 0 |
| CLI run prediction.dcraft | PASS, exit 0 |
| sbt test writeClasspath | PASS по предоставленному пользователем выводу: 6/6 |
| CLI run sample.dcraft | PASS по предоставленному пользователем выводу: (4,11), (5,13), (6,15) |

Начальный fmt --check обнаружил несоответствие форматированию. Применён
rustfmt; изменения Rust-файлов в этой итерации только форматирующие.
Cargo.lock существует. Для обхода устаревшего PATH использован прямой путь
к установленному toolchain и DATACRAFT_JAVA; пользовательские переменные не менялись.
Предупреждение о canonicalization пользовательского home не помешало командам;
оно не является предупреждением Clippy исходного кода.

ML-примеры запускались настоящим cargo run --locked --offline -p datacraft-cli
с run examples/pipelines/<имя>.dcraft. Фактический вывод регрессии:

```text
Intercept: 2.9999999999999987
feature[0]: 2.0000000000000004
training rmse: 3.1297001304428877e-15
training r2: 1
```

Predict: quantity 1..5, amount 5/7/9/11/13, prediction
4.999999999999999/7/9/11/13. Оракул задаётся независимо: amount = 2*quantity+3.
Логи последних CLI-прогонов: .tools/baseline-checks/{regression,prediction}.{stdout,stderr}.log.
Малые наблюдаемые ошибки не являются выводом допустимой погрешности solver.

NOT_RUN: отдельная приёмка Linux/macOS, новый запуск удалённого CI,
полная проверка внешних фактов зависимостей, независимое поведенческое review.
ASSUMPTION: порог 1e-6 ML-тестов ещё не обоснован анализом solver;
эквивалентность старым нодам не заявляется. Отдельные команды CLI validate и
inspect в этой итерации не запускались.

Далее сохранена история первоначальных ограничений окружения; её статусы
не заменяют актуальную таблицу выше.

## Обновление: исправлен Windows Parquet fixture

Ниже сохранён первоначальный отчёт окружения; он не описывает все последующие
изменения установки инструментов. Rust 1.99.1, Cargo, rustfmt, Clippy и JDK
21.0.13 впоследствии найдены; минимальная Rust-программа собрана и запущена.
Это не означает прохождения cargo test всего workspace.

Пользователь запустил sbt 1.10.11: WorkerSuite первоначально дал 5 PASS / 1 FAIL.
XML-отчёт показал ошибку Hadoop Windows permissions при записи Parquet fixture.
Подготовка fixture заменена на Parquet LocalOutputFile через Java NIO.
Исправленный тест скомпилирован Scala 2.13.16; JUnitCore выполнил все шесть
тестов WorkerSuite с настоящим Spark 4.0.1: OK (6 tests), exit 0.
Подробности и ограничения: [PARQUET_SOURCE_TEST.md](PARQUET_SOURCE_TEST.md).
Повторный sbt test writeClasspath и Rust–worker CLI этим прогоном не проверены.

## Выполнено успешно

- Python `tomllib` прочитал все пять `Cargo.toml` без ошибок синтаксиса TOML.
- Python `json` прочитал три примера `.dcraft`; проверены уникальность UUID,
  наличие designated output, порядок зависимостей и существование CSV по
  относительному пути каждого проекта.
- Проверен CSV: 12 строк, `amount = 2 * quantity + 3`; ожидаемый результат
  фильтра `quantity >= 4` с limit 3 — quantity 4, 5, 6.
  Это проверка исходных fixtures на Python, не исполнение Spark.
- Найден и запущен Java runtime:
  `C:\Program Files\JetBrains\CLion 2025.1.1\jbr\bin\java.exe` — OpenJDK 21.0.6.

## Не удалось запустить

| Команда | Результат |
|---|---|
| `cargo fmt --all -- --check` | `cargo` не найден |
| `cargo check --workspace` | `cargo` не найден |
| `cargo test --workspace` | `cargo` не найден |
| `cargo clippy --workspace --all-targets -- -D warnings` | `cargo` не найден |
| `sbt test` | `sbt` не найден |

Ни одна проверка компиляции Rust/Scala не отмечается как пройденная.
Форматирование Rust ещё требует запуска rustfmt. Cargo.lock отсутствует.
Команды `datacraft validate/run/inspect` и реальный Spark pipeline не запускались:
для них сначала нужны собранные CLI и worker.

## Блокеры окружения

- Rust/rustup не найдены в PATH и стандартных пользовательских каталогах.
- sbt и Spark не найдены в PATH.
- Загрузка Rust manifest через PowerShell не установила соединение.
- Запрос Maven через Python завершился `WinError 10013`: доступ к сокету запрещён.
- `wsl --list --quiet` завершился `Wsl/EnumDistros/Service/E_ACCESSDENIED`.

Привилегии не повышались, системные настройки не менялись.

## Подготовленные, но не выполненные тесты

- Rust core: DAG, циклы, отсутствующие ссылки, порты, несовместимые типы,
  обязательные входы, повторные соединения/UUID, параметры, выбор выхода,
  замыкание зависимостей и детерминированный порядок.
- Rust project: JSON round-trip, канонический порядок, версии, повреждённый
  JSON, сохранение/перезапись и защита будущих версий, относительные пути.
- Scala: CSV pipeline с точными строками и схемой, коэффициенты 3 и 2,
  обучающие метрики, Predict, ошибки колонок/файлов/фильтра, Parquet.
- CI: Rust fmt/check/test/Clippy, Scala tests и сквозные CLI-команды.

CI только описан в файле workflow; удалённый запуск не производился.
