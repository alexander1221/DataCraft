# DataCraft

Нативная local-first среда визуальной обработки данных. Первый этап — headless
основание на Rust и локальный Scala worker с Apache Spark. Добавлен первый
desktop-клиент на eframe/egui для просмотра графа и запуска preview.

**Статус:** исходный код первой вертикали написан. Все шесть тестов Scala worker
прошли с настоящим Spark на Windows/JDK 21 после исправления Parquet fixture.
Rust fmt/check/test/Clippy прошли; CSV preview, обучение и предсказание
проверены через связку Rust CLI → worker на Windows/JDK 21.
Это ещё не готовый релиз.
Полный список выполненных проверок — в [docs/VALIDATION.md](docs/VALIDATION.md).

## Возможности исходной реализации

- UUID-идентификаторы нод, типизированные порты `Table` и `Model`, проверка DAG,
  обязательных входов, параметров, повторных соединений и ссылок.
- Единый NodeSpec для девяти нод: порты/cardinality, параметры, defaults и
  ограничения. Discovery через `datacraft nodes --json` без JVM.
- JSON-проекты `.dcraft` версии 1 с координатами нод и относительными путями.
- Сохранение черновиков с незаполненными параметрами/входами; диагностика
  через `validate --draft` и запуск готовой ветки независимо от неполной.
- Headless Session с типизированными командами, атомарными пакетами правок,
  ограниченными undo/redo и dirty-checkpoint. CLI использует Session для
  открытия и исполнения; список контрактов доступен через `commands --json`.
- Один фоновый job, Job ID, polling статуса/событий, отмена и timeout.
  Вычисления работают на snapshot проекта; редактирование остаётся доступным.
- CSV, Parquet, Filter (Spark SQL expression), Select Columns, Limit, Table Preview.
- Assemble Features для числовых колонок, Linear Regression, Predict.
- Таблицы коэффициентов и **обучающих** метрик RMSE/R². Это не оценка на test set.
- CLI `validate`, `run`, `inspect`; ошибки приводят к ненулевому exit code.
- `cargo xtask`: контроль зависимостей слоёв, реестров команд/нод и эталонных
  pipeline через настоящий Spark с проверкой схем и значений.
- Desktop: New/Open/Save, палитра девяти типов нод, создание/удаление, canvas
  со связями, выбор/перемещение, редактируемые параметры с Apply/Revert,
  Connect/Disconnect, Undo/Redo, Run/Cancel и таблица результатов.

## Требования и сборка

Нужны Rust stable с Cargo, rustfmt и Clippy, JDK 17 или 21, sbt 1.10.11.
Для MSVC-сборки Rust на Windows нужны C++ Build Tools и Windows SDK.
Spark 4.0.1 / Scala 2.13.16 подключаются через sbt; отдельные Livy, Python
и установленный Spark не требуются. Первая сборка требует доступа к Cargo/Maven
репозиториям; после загрузки зависимостей вычисления выполняются локально.

Остальные зависимости: serde 1, serde_json 1, uuid 1, thiserror 2, tempfile 3,
clap 4; Scala JSON — ujson 4.2.1, тесты — MUnit 1.0.4. Точные разрешённые
версии Rust-зависимостей зафиксированы в Cargo.lock.
Desktop использует eframe/egui 0.33.3 с Glow, X11/Wayland backends; версия
выбрана по проверенному API. Дополнительная установка GUI-библиотек на Windows
не потребовалась. Native запуск на других ОС этой итерацией не подтверждён.

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
новый job создаёт новый процесс.

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

- В desktop пути вводятся в строку; native file picker отсутствует.
  Порты соединяются двумя нажатиями (выход → вход), drag-to-connect пока нет.
  Train/test split и категоризация пока отсутствуют.
- Preview ограничен 1000 строками и ответом 8 MiB. Это не ограничение памяти
  отдельной широкой строки, парсера или обучения модели.
- Источники и промежуточные таблицы остаются внутри JVM; Rust получает только
  схемы и ограниченные результаты. Spark может читать весь CSV при inferSchema.
- В Session один активный job. Каждый job создаёт JVM; между jobs нет кэша
  DataFrame/моделей или пула процессов.
- События сообщают этапы выполнения, а не процент/строки Spark. Ctrl+C-handler
  CLI пока отсутствует; cancellation/status доступны через API Session.
- Timeout и отмена зависят от возможности ОС завершить процесс; terminal status
  публикуется после cleanup. Drop запрашивает best-effort stop; hosts должны
  дождаться shutdown перед выходом, если нужно подтверждение очистки.
- Модели не сохраняются на диск. Параметры preprocessing надо переносить вместе
  с моделью, когда появятся обучаемые преобразования.
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
Следующие этапы: [docs/ROADMAP.md](docs/ROADMAP.md).

Правила участия и проверки перед PR: [CONTRIBUTING.md](CONTRIBUTING.md).
Код проекта распространяется под MIT OR Apache-2.0 — см. LICENSE-MIT и
LICENSE-APACHE. DataCraft не является официальным продуктом ArtCraft.
