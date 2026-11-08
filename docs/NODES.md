# Реестр встроенных нод

## §1. Задача и инварианты

NodeSpec — единый декларативный контракт встроенных нод для создания,
статической валидации, discovery и будущего GUI. Реестр находится в core,
не зависит от Spark/UI. Исполнение остаётся в Scala worker.
Существующие kind, поля JSON, UUID и идентификаторы портов сохраняются.

## §2. Алгоритм

NodeKind выбирает свой NodeSpec. Создание конфигурации по kind применяет
заявленные defaults, отклоняет неизвестные/отсутствующие параметры, проверяет
типы, диапазоны и межпараметрические ограничения, затем десериализует NodeKind.
Валидация существующего NodeKind использует те же правила. Graph получает
порты и признак выходной ноды из NodeSpec.

## §3. Применения

Реестр предназначен для инспектора параметров, создания нод, отображения
портов и проверки проектов. CLI nodes выводит metadata, включая параметры,
defaults и ограничения, в читаемом виде либо JSON. Это не новый backend
и не система загрузки сторонних плагинов.

## §4. Спецификация

NodeSpec: стабильный id, label, category, input/output PortSpec,
ParameterSpec и selectable_output. Входы обязательные, single-input;
выходы допускают fan-out. Это явно выражено cardinality required_single/many.
Имена портов уникальны внутри своего направления.
Table и Model — существующие типы портов.

ParameterSpec: name, label, тип, optional default и список ограничений.
Без default значение требуется от вызывающего; пустое значение не считается
отсутствующим и отдельно проверяется. Диапазоны включают обе границы.
Строки проверяются на непустоту после trim, но сохраняются без нормализации.
Списки колонок непустые, элементы непустые и уникальны при точном сравнении.
Float обязан быть конечным; integer ограничивается типом u32 и заданным range.
Проверки наличия колонок и Spark SQL expressions остаются schema-aware.

| kind | Порты input → output | Defaults (остальные поля обязательны) |
|---|---|---|
| csv_source | — → Table | header=true, infer_schema=true |
| parquet_source | — → Table | — |
| filter | Table → Table | — |
| select_columns | Table → Table | — |
| limit | Table → Table | rows=100 |
| assemble_features | Table → Table | output_column=features |
| linear_regression | Table → Model | features_column=features, fit_intercept=true, max_iter=100, reg_param=0, elastic_net_param=0 |
| predict | Table + Model → Table | — |
| table_preview | Table → — | rows=100 |

Limit rows: 1..2147483647; preview rows: 1..1000; max_iter:
1..2147483647; reg_param >= 0; elastic_net_param: 0..1.
Label и features columns различны; output_column не входит в columns.
Только table_preview и linear_regression являются выбираемыми выходами.
Defaults применяются при явном создании, не изменяют загрузку старых .dcraft.

## §5. Реализация

Код: crates/datacraft-core/src/registry.rs и NodeKind в src/lib.rs.
Проверка реестра выявляет повторяющиеся IDs, порты, параметры, неверные defaults
и ссылки ограничений на неизвестные параметры. Реестр сериализуется для discovery;
обратная загрузка пользовательского metadata не поддерживается.

## §6. Замечания

Enum остаётся типизированным; правила и metadata не дублируются в CLI.
GraphError сохраняет прежний формат; отдельный ParameterError содержит kind,
имя параметра и причину. Scala повторно проверяет недоверенную границу протокола.
Семантика черновиков и create_draft_config описаны в PROJECT_FORMAT.md.
Session и команды редактирования описаны в COMMANDS.md.

## §7. Приёмка

Тесты сверяют публичный контракт с независимыми таблицами ожидаемых портов,
полей JSON, defaults и диапазонов; проверяют malformed metadata и старые fixtures.
Далее запускаются Rust fmt/check/test/Clippy и прежние CLI/Spark-примеры.
Фактический результат: 21 Rust-тест, fmt/check/Clippy и три прежних Spark-примера
прошли на Windows; детали и неисполненные проверки — в docs/VALIDATION.md.

Пример discovery: `cargo run --locked -p datacraft-cli -- nodes --json`.
Создание Rust-конфигурации:

```rust
let spec = datacraft_core::find_node("table_preview").ok_or("unknown node")?;
let config = spec.create_config(serde_json::Map::new())?; // rows = 100
config.validate_parameters()?;
```

Это фрагмент внутри функции, возвращающей совместимый Result; он не запускает JVM.

## §8′. Устойчивость

Ошибки пользователя возвращаются как Result, без panic. Реестр конечный,
девять нод; поиск линейный. Parameter validation не читает datasets, не запускает
JVM и не оценивает SQL. Проверка непустого local path не гарантирует существование
файла. Реестр не доказывает универсальную bounded-memory работу Spark.
