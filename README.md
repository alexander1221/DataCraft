# DataCraft

Нативная local-first среда визуальной обработки данных. Первый этап — headless
основание на Rust и локальный Scala worker с Apache Spark. GUI ещё не реализован.

**Статус:** исходный код первой вертикали написан. Все шесть тестов Scala worker
прошли с настоящим Spark на Windows/JDK 21 после исправления Parquet fixture.
Rust fmt/check/test/Clippy прошли; CSV preview, обучение и предсказание
проверены через связку Rust CLI → worker на Windows/JDK 21.
Это ещё не готовый релиз.
Полный список выполненных проверок — в [docs/VALIDATION.md](docs/VALIDATION.md).

## Возможности исходной реализации

- UUID-идентификаторы нод, типизированные порты `Table` и `Model`, проверка DAG,
  обязательных входов, параметров, повторных соединений и ссылок.
- JSON-проекты `.dcraft` версии 1 с координатами нод и относительными путями.
- CSV, Parquet, Filter (Spark SQL expression), Select Columns, Limit, Table Preview.
- Assemble Features для числовых колонок, Linear Regression, Predict.
- Таблицы коэффициентов и **обучающих** метрик RMSE/R². Это не оценка на test set.
- CLI `validate`, `run`, `inspect`; ошибки приводят к ненулевому exit code.

## Требования и сборка

Нужны Rust stable с Cargo, rustfmt и Clippy, JDK 17 или 21, sbt 1.10.11.
Для MSVC-сборки Rust на Windows нужны C++ Build Tools и Windows SDK.
Spark 4.0.1 / Scala 2.13.16 подключаются через sbt; отдельные Livy, Python
и установленный Spark не требуются. Первая сборка требует доступа к Cargo/Maven
репозиториям; после загрузки зависимостей вычисления выполняются локально.

Остальные зависимости: serde 1, serde_json 1, uuid 1, thiserror 2, tempfile 3,
clap 4; Scala JSON — ujson 4.2.1, тесты — MUnit 1.0.4. Точные разрешённые
версии Rust-зависимостей зафиксированы в Cargo.lock.

Следующие команды предназначены для подготовленного окружения. Какие именно
проверки выполнены, указано в docs/VALIDATION.md:

```sh
cargo build --workspace
cd workers/spark
sbt test writeClasspath
cd ../..
cargo run -p datacraft-cli -- validate examples/pipelines/sample.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/sample.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/regression.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/prediction.dcraft
cargo run -p datacraft-cli -- inspect examples/data/sales.csv --rows 5
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

`sample.dcraft`: CSV → Filter → Select Columns → Limit → Table Preview.
Ожидаемые quantity: 4, 5, 6. `regression.dcraft` обучает зависимость
`amount = 2 * quantity + 3`; `prediction.dcraft` применяет модель и показывает
первые пять предсказаний. Шесть Scala-тестов прошли; сквозные примеры CSV,
регрессии и Predict также успешно выполнены на Windows.

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd workers/spark
sbt test
```

## Ограничения

- Desktop-приложение, редактор графа, train/test split и категоризация пока
  отсутствуют. Команды запуска desktop нет.
- Preview ограничен 1000 строками и ответом 8 MiB. Это не ограничение памяти
  отдельной широкой строки, парсера или обучения модели.
- Источники и промежуточные таблицы остаются внутри JVM; Rust получает только
  схемы и ограниченные результаты. Spark может читать весь CSV при inferSchema.
- В одной сессии выполняется один запрос. Между запросами SparkSession живёт,
  но DataFrame и модели не сохраняются. CLI создаёт сессию на один запуск.
- Протокол блокирующий; нет timeout, прогресса и интерактивной отмены CLI.
  Есть метод завершения worker для будущего lifecycle-контроля.
- Модели не сохраняются на диск. Параметры preprocessing надо переносить вместе
  с моделью, когда появятся обучаемые преобразования.
- Запись проекта требует валидного завершённого графа. Сохранение черновиков
  с неподключёнными нодами нужно добавить перед GUI.
- Численное обоснование допуска 1e-6 в ML-тестах ещё не оформлено;
  эквивалентность исходным Spark-нодам не доказана.

Нет телеметрии, аккаунтов, Livy, внешнего оркестратора и script-нод.
Spark использует локальные TCP-соединения и временные файлы; JVM не является
песочницей. Открывайте проекты из доверенных источников: они задают пути чтения.

Архитектура: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
Следующие этапы: [docs/ROADMAP.md](docs/ROADMAP.md).

Правила участия и проверки перед PR: [CONTRIBUTING.md](CONTRIBUTING.md).
Код проекта распространяется под MIT OR Apache-2.0 — см. LICENSE-MIT и
LICENSE-APACHE. DataCraft не является официальным продуктом ArtCraft.
