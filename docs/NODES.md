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
отсутствующим и отдельно проверяется. NumberRange включает обе границы;
OpenNumberRange исключает их (train_fraction строго между 0 и 1).
Строки проверяются на непустоту после trim, но сохраняются без нормализации.
Списки колонок непустые, элементы непустые и уникальны при точном сравнении.
Float обязан быть конечным; integer ограничивается типом u32 и заданным range.
Проверки наличия колонок и Spark SQL expressions остаются schema-aware.

| kind | Порты input → output | Defaults (остальные поля обязательны) |
|---|---|---|
| csv_source | — → Table | header=true, infer_schema=true |
| parquet_source | — → Table | — |
| json_source | — → Table | path обязателен; NDJSON |
| train_test_split | table → train, test (Table) | train_fraction=0.8, seed=42; key_columns обязательны |
| make_design_matrix | table → table, model | features_column=features, label_column=label, keep_unknown=false; formula обязательна |
| index_categoricals | table → table, model | keep_unknown=true; columns обязательны |
| reference_code_categoricals | table → table, model | keep_unknown=false, drop_last=true; columns обязательны |
| transform | table + model → table | fitted preprocessing, без повторного fit |
| filter | Table → Table | — |
| select_columns | Table → Table | — |
| limit | Table → Table | rows=100 |
| assemble_features | Table → Table | output_column=features |
| linear_regression | Table → Model | features_column=features, fit_intercept=true, max_iter=100, reg_param=0, elastic_net_param=0 |
| generalized_linear_regression | Table → Model | family=gaussian, link=default, features_column=features, fit_intercept=true, max_iter=25, reg_param=0 |
| decision_tree_regressor | Table → Model | features_column=features, max_depth=5, max_bins=32, min_instances_per_node=1, seed=42 |
| random_forest_regressor | Table → Model | как Tree; num_trees=20, bootstrap=true, feature_subset_strategy=auto |
| regression_metrics | Table → — | prediction_column=prediction; label_column обязателен |
| logistic_regression | Table → Model | features_column=features, fit_intercept=true, max_iter=100, reg_param=0, elastic_net_param=0, threshold=0.5; binary |
| linear_svc | Table → Model | features_column=features, fit_intercept=true, max_iter=100, reg_param=0, threshold=0; binary L2 SVM |
| decision_tree_classifier | Table → Model | как Tree Regressor; impurity=gini (gini/entropy) |
| random_forest_classifier | Table → Model | как Forest Regressor; impurity=gini |
| classification_metrics | Table → — | prediction_column=prediction; label_column обязателен |
| feature_selection | Table → — | categorical_columns=[], num_buckets=10, relative_error=0.005; label_column/columns обязательны |
| feature_selection_pipeline | Table → Table + Model | как Feature Selection; num_top_features=1, output_column=selected_features, keep_unknown=true |
| random_forest_pipeline | Table → Table + Model | как Forest Regressor; task=classification, num_top_features=1, output_column=selected_features |
| predict | Table + Model → Table | — |
| table_preview | Table → — | rows=100 |

Limit rows: 1..2147483647; preview rows: 1..1000; max_iter:
1..2147483647; reg_param >= 0; elastic_net_param: 0..1.
Label и features columns различны; output_column не входит в columns.
Выбираемые выходы: table_preview, linear_regression, regression_metrics,
classification_metrics и все три ноды отбора (отчёт).
Новые regression/classification estimator-ноды передают Model в Predict, прямой model report для них
пока не предусмотрен. Метрики и допустимые GLM family/link: [REGRESSION.md](REGRESSION.md).
Classifier domains, probability/margin thresholds и weighted метрики:
[CLASSIFICATION.md](CLASSIFICATION.md).
Отбор и статистические ограничения: [FEATURE_SELECTION.md](FEATURE_SELECTION.md).
StringList defaults поддерживают [], ItemsIn задаёт подмножество другого списка,
MaxItems ограничивает размер. Для selection columns лимит 1000 проверяется до JVM.
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
Актуальные результаты Rust/Scala и сквозных примеров, а также неисполненные
проверки перечислены в docs/VALIDATION.md. Контракт подготовки признаков:
[PREPROCESSING.md](PREPROCESSING.md).

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
27 нод; поиск линейный. Parameter validation не читает datasets, не запускает
JVM и не оценивает SQL. Проверка непустого local path не гарантирует существование
файла. Реестр не доказывает универсальную bounded-memory работу Spark.
