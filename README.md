# DataCraft

Нативная local-first среда визуальной обработки данных. Первый этап — headless
основание на Rust и локальный Scala worker с Apache Spark. Добавлен первый
desktop-клиент на eframe/egui для просмотра графа и запуска preview.

**Статус:** работают визуальный редактор, подготовка/отбор признаков, регрессия и классификация.
Проверены 131 Rust-тест, 60 Scala/MUnit-тестов и 9 GUI/Spark-сценариев
на Windows/JDK 21, включая настоящий HDFS для Text/Parquet import/export.
Rust fmt/check/Clippy и архитектурные проверки прошли; CSV/JSON preview,
обучение, предсказание и fit/transform проверены через Rust CLI → worker.
В текущей sandbox sbt заблокирован доступом к boot-кэшу; Scala скомпилирована
установленным компилятором, тесты исполнены через JUnit. Обычная сборка ниже — sbt.
Это ещё не готовый релиз.
Полный список выполненных проверок — в [docs/VALIDATION.md](docs/VALIDATION.md).

## Возможности исходной реализации

- UUID-идентификаторы нод, типизированные порты `Table` и `Model`, проверка DAG,
  обязательных входов, параметров, повторных соединений и ссылок.
- Единый NodeSpec для 32 нод: порты/cardinality, параметры, defaults и
  ограничения. Discovery через `datacraft nodes --json` без JVM.
- JSON-проекты `.dcraft` версии 1 с координатами нод и относительными путями.
- Сохранение черновиков с незаполненными параметрами/входами; диагностика
  через `validate --draft` и запуск готовой ветки независимо от неполной.
- Headless Session с типизированными командами, атомарными пакетами правок,
  ограниченными undo/redo и dirty-checkpoint. CLI использует Session для
  открытия и исполнения; список контрактов доступен через `commands --json`.
- Один фоновый job, Job ID, polling статуса/событий, отмена и timeout.
  Вычисления работают на snapshot проекта; редактирование остаётся доступным.
- CSV, Parquet, NDJSON, Filter (Spark SQL expression), Select Columns, Limit, Table Preview.
- HDFS Text import/export: WebHDFS, none/Simple, настройки формата и remote browser.
- Локальный Parquet Export и HDFS Parquet import/export: один бинарный файл,
  Snappy/Gzip/Zstd/uncompressed, явные overwrite и create_parents.
  Контракты: [HDFS_TEXT](docs/HDFS_TEXT.md), [PARQUET_IO](docs/PARQUET_IO.md).
- Train/Test Split с seed и стабильными ключами; RFormula Design Matrix,
  Index Categoricals, Reference Code Categoricals и fitted Transform без повторного fit.
- Assemble Features для чисел и ML-векторов, Linear Regression, Predict.
- Generalized Linear Regression (gaussian/binomial/poisson/gamma), Decision Tree
  Regressor и Random Forest Regressor; общий Predict для регрессионных моделей.
- Regression Metrics: RMSE/MSE/MAE/R² на поданной таблице предсказаний.
  Новые примеры оценивают test; при постоянном target R² возвращается как null.
- Logistic Regression и Linear SVM для binary 0/1; Decision Tree Classifier и
  Random Forest Classifier также для multiclass. Общий Predict принимает classifiers.
- Classification Metrics: accuracy, weighted precision/recall/F1 на поданном наборе.
- Feature Selection: χ²-отчёт для categorical target; Feature Selection Pipeline:
  train-only top-k и fitted Transform. Random Forest Pipeline отбирает компоненты
  ML-вектора по importance для classification/regression.
- Таблицы коэффициентов и **обучающих** метрик RMSE/R². Это не оценка на test set.
- CLI `validate`, `run`, `inspect`; ошибки приводят к ненулевому exit code.
- `cargo xtask`: контроль зависимостей слоёв, реестров команд/нод и эталонных
  pipeline через настоящий Spark с проверкой схем и значений.
- Desktop: New/Open/Save, палитра 32 типов нод, создание/удаление, canvas
  со связями, выбор/перемещение, редактируемые параметры с Apply/Revert,
  Connect/Disconnect, Undo/Redo, Run/Cancel и таблица результатов.
- Асинхронные системные Open/Save as, Browse для CSV/Parquet/JSON и переименование
  проекта с Undo/Redo. Ручной ввод путей сохраняется.
- Arrange разносит ветки графа одним Undo; Move/Rename сохраняют актуальность
  результата. Примеры fit/transform имеют разнесённую раскладку.

## Требования и сборка

Нужны Rust stable с Cargo, rustfmt и Clippy, JDK 17 или 21, sbt 1.10.11.
Для MSVC-сборки Rust на Windows нужны C++ Build Tools и Windows SDK.
Spark 4.0.1 / Scala 2.13.16 подключаются через sbt; отдельные Livy, Python
и установленный Spark не требуются. Первая сборка требует доступа к Cargo/Maven
репозиториям; после загрузки зависимостей вычисления выполняются локально.

Остальные зависимости: serde 1, serde_json 1, uuid 1, thiserror 2, tempfile 3,
clap 4; Scala JSON — ujson 4.2.1, тесты — MUnit 1.0.4. Точные разрешённые
версии Rust-зависимостей зафиксированы в Cargo.lock.
Desktop использует eframe/egui 0.33.3 с Glow, X11/Wayland backends и rfd 0.18.0
(MIT, Rust 1.92+); версия
выбрана по проверенному API. Дополнительная установка GUI-библиотек на Windows
не потребовалась. Linux picker использует XDG Desktop Portal с GTK/GNOME/KDE
backend либо Zenity и системный libdbus. Подробности и границы проверки —
[FILE_DIALOGS.md](docs/FILE_DIALOGS.md), [VALIDATION.md](docs/VALIDATION.md).

Следующие команды предназначены для подготовленного окружения. Какие именно
проверки выполнены, указано в docs/VALIDATION.md:

```sh
cargo build --workspace
cd workers/spark
sbt test writeClasspath
cd ../..
cargo run -p datacraft-cli -- validate examples/pipelines/sample.dcraft
cargo run -p datacraft-cli -- nodes --json
cargo run -p datacraft-cli -- commands --json
cargo run -p datacraft-cli -- validate examples/pipelines/draft.dcraft --draft
cargo run -p datacraft-cli -- run examples/pipelines/draft.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/sample.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/regression.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/prediction.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/index_categoricals.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/reference_code.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/design_matrix.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/train_test.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/glm_regression.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/decision_tree.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/random_forest.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/random_forest.dcraft --output 00000000-0000-0000-0000-000000000009
cargo run -p datacraft-cli -- run examples/pipelines/logistic_regression.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/linear_svc.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/decision_tree_classifier.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/random_forest_classifier.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/feature_selection.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/feature_selection_pipeline.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/random_forest_pipeline.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/random_forest_pipeline.dcraft --output 00000000-0000-0000-0000-000000000011
cargo run -p datacraft-cli -- inspect examples/data/features.jsonl --rows 5
cargo run -p datacraft-cli -- inspect examples/data/sales.csv --rows 5
cargo run -p datacraft-desktop -- examples/pipelines/sample.dcraft
```

`writeClasspath` компилирует worker и записывает абсолютный classpath в
`workers/spark/target/classpath.txt`. При переносе каталога выполните команду
заново. CLI запускайте из корня workspace либо задавайте путь явно:

```sh
cargo run -p datacraft-cli -- --java /path/to/java --worker-classpath /path/to/classpath.txt run /path/to/project.dcraft
```

Альтернативы флагам: `DATACRAFT_JAVA`, `DATACRAFT_WORKER_CLASSPATH`.
Java-путь указывает на исполняемый файл, classpath-путь — на текстовый файл.
Если выходов несколько, задайте `--output UUID` или `output_node` проекта.

## Примеры и проверки

Deadline включает запуск JVM, по умолчанию 300 секунд:

```sh
cargo run -p datacraft-cli -- run examples/pipelines/sample.dcraft --timeout-seconds 60
cargo run -p datacraft-engine --example job_lifecycle -- examples/pipelines/sample.dcraft
```

Второй пример проверяет отмену настоящего JVM, следующий запуск и устаревание
результата при редактировании во время работы. CLI ждёт завершения фонового job;
Desktop отдельно опрашивает status/events. Отмена завершает локальный JVM,
следующий запуск восстанавливает worker. Кнопки на нодах используют постоянный
worker и поузловой кэш на время открытого проекта: [NODE_EXECUTION.md](docs/NODE_EXECUTION.md).

`sample.dcraft`: CSV → Filter → Select Columns → Limit → Table Preview.
Ожидаемые quantity: 4, 5, 6. `regression.dcraft` обучает зависимость
`amount = 2 * quantity + 3`; `prediction.dcraft` применяет модель и показывает
первые пять предсказаний. Шесть Scala-тестов прошли; сквозные примеры CSV,
регрессии и Predict также успешно выполнены на Windows.

```sh
cargo xtask layers
cargo xtask commands
cargo xtask nodes
cargo xtask fixtures
cargo xtask ci
cargo xtask ci --with-spark
```

`ci` выполняет fmt/check/test/Clippy и проверки архитектуры; `--with-spark`
добавляет `sbt test writeClasspath`, эталонные pipeline и отмену/восстановление JVM.
Отдельный `fixtures` требует собранного worker. Oracle:
`examples/expected/pipelines.json`; ML-допуск 1e-6 остаётся явным допущением.
`layers` учитывает зависимости всех платформ, включая dev/build и транзитивные;
первый запуск может загрузить пакеты, не нужные для сборки на текущей ОС.
`--offline` используйте после заполнения Cargo-кэша. Отсутствие пакетов или JVM
завершает соответствующую проверку ошибкой. Подробнее:
[ARCHITECTURE_CHECKS.md](docs/ARCHITECTURE_CHECKS.md).

## Ограничения

- На Linux системный picker требует desktop portal/backend либо Zenity;
  при отсутствии выбора доступны Open path/Save path и ручное поле источника.
  Порты соединяются двумя нажатиями (выход → вход), drag-to-connect пока нет.
- Preview ограничен 1000 строками и ответом 8 MiB. Это не ограничение памяти
  отдельной широкой строки, парсера или обучения модели.
- Источники и промежуточные таблицы остаются внутри JVM; Rust получает только
  схемы и ограниченные результаты. Spark может читать весь CSV при inferSchema.
- В Session один активный job. Обычный Run project создаёт отдельный JVM.
  ▶ выполняет только ноду с готовыми кэшированными входами, ⏩ — её и потомков.
  Для этих кнопок worker сохраняет модели и MEMORY_AND_DISK таблицы между jobs.
  Материализация читает весь результат ноды; кэш не сохраняется между открытиями.
- События сообщают этапы выполнения, а не процент/строки Spark. Ctrl+C-handler
  CLI пока отсутствует; cancellation/status доступны через API Session.
- Timeout и отмена зависят от возможности ОС завершить процесс; terminal status
  публикуется после cleanup. Drop запрашивает best-effort stop; hosts должны
  дождаться shutdown перед выходом, если нужно подтверждение очистки.
- Модели и fitted preprocessing сохраняются только в памяти worker,
  экспорт моделей на диск пока отсутствует.
- Сохранение черновиков
  допускает неподключённые ноды и незаполненные параметры, но отклоняет циклы,
  битые ссылки, несовместимые порты и нечисловые координаты/float-параметры.
- Численное обоснование допуска 1e-6 в ML-тестах ещё не оформлено;
  эквивалентность исходным Spark-нодам не доказана.

Нет телеметрии, аккаунтов, Livy, внешнего оркестратора и script-нод.
Spark использует локальные TCP-соединения и временные файлы; JVM не является
песочницей. Открывайте проекты из доверенных источников: они задают пути чтения.

Архитектура: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
Контракты нод: [docs/NODES.md](docs/NODES.md).
Формат и проверки черновиков: [docs/PROJECT_FORMAT.md](docs/PROJECT_FORMAT.md).
Session и команды: [docs/COMMANDS.md](docs/COMMANDS.md).
Фоновые задания и отмена: [docs/EXECUTION.md](docs/EXECUTION.md).
Первый desktop и управление canvas: [docs/DESKTOP.md](docs/DESKTOP.md).
Редактор нод, параметров и связей: [docs/GRAPH_EDITOR.md](docs/GRAPH_EDITOR.md).
Файловые диалоги и Rename: [docs/FILE_DIALOGS.md](docs/FILE_DIALOGS.md).
Следующие этапы: [docs/ROADMAP.md](docs/ROADMAP.md).

Правила участия и проверки перед PR: [CONTRIBUTING.md](CONTRIBUTING.md).
Код проекта распространяется под MIT OR Apache-2.0 — см. LICENSE-MIT и
LICENSE-APACHE. DataCraft не является официальным продуктом ArtCraft.
