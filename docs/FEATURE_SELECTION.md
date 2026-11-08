# Отбор признаков

## §1. Задача

Feature Selection возвращает bounded отчёт Pearson χ² по сырым predictors.
Feature Selection Pipeline обучает top-k отбор на train и возвращает table/model.
Random Forest Pipeline выбирает компоненты готового ML-вектора по importance.
Обе fitted модели применяются к test через существующий Transform без нового fit.
Исходная локальная Spark-спецификация используется как описание задач, не как код.

## §2. Алгоритм

Параметр columns задаёт порядок кандидатов; categorical_columns — подмножество.
Остальные numeric columns дискретизируются QuantileDiscretizer для χ².
StringIndexer с alphabetAsc кодирует категории и target. Fit использует только
train; Pearson test возвращает statistic, degrees_of_freedom и p_value.
Pipeline выбирает минимальные конечные p-values, tie-break по исходному индексу.
Порядок выбранного вектора — исходный columns, не рейтинг p-value. Вектор хранит
исходные continuous values и fitted category indices; bins в него не попадают.
Label indexer/discretizer для статистики не нужны при применении модели к test.

RF Pipeline обучает classification/regression forest на train vector, сортирует
importance по убыванию и исходному индексу, затем сохраняет VectorSlicer с
проверкой исходной размерности. RF сам не становится Predict-моделью этой ноды:
выход model — только отбор. Оригинальный вектор и metadata сохраняются.

## §3. Применения

Raw train → Selection Pipeline.table → Classifier;
Raw test + Selection Pipeline.model → Transform → Predict → Metrics.
Для RF: train Design Matrix → RF Pipeline → Classifier; test проходит тот же
fitted Design Matrix, затем RF selection Transform. Test не выбирает признаки.
Ноды Pipeline можно выбрать выходом для отчёта; Feature Selection — только отчёт.

## §4. Контракт

kind feature_selection: table→report; label_column, columns обязательны.
categorical_columns=[], num_buckets=10 (2..1000), relative_error=0.005 (0..1).
Не более 1000 уникальных непустых кандидатов; target не входит в columns.
Categorical subset unique, string/numeric; continuous numeric finite.
Train null/NaN/Infinity отвергаются; target имеет 2..1000 категорий, каждый
categorical predictor — до 1000 категорий. Непрерывный target этим тестом не
поддерживается: его надо явно подготовить как categorical response.
Report: feature, statistic, degrees_of_freedom, p_value; при df=0 p_value=null.

feature_selection_pipeline: те же параметры плюс num_top_features=1 (1..1000),
output_column=selected_features, keep_unknown=true. table→table/model, selectable
report добавляет selected. K не превосходит числа кандидатов с определённым p.
Transform требует все candidate columns, но не target. Unknown/null categorical
идут в extra bucket при keep_unknown; иначе ошибка. Выбранные continuous
проверяются на finite values. Временные колонки удаляются, исходные сохранены.
Output name отличается от target/candidates и не перезаписывает другую колонку.

random_forest_pipeline: label_column обязателен, features_column=features,
output_column=selected_features, num_top_features=1, task=classification либо
regression. Параметры forest как у существующего RF: depth=5, bins=32,
min_instances=1, seed=42, num_trees=20, bootstrap=true, feature_subset_strategy=auto.
Размер вектора 1..1000; K<=dimension. Report: feature_index, feature, importance,
selected. Metadata name используется при наличии, иначе feature[i].
Selection model не принимает test vector другой длины и не является Predictor.
Форматы .dcraft/protocol остаются v1; модель живёт в job и не сохраняется.

## §5. Реализация

NodeKind/NodeSpec в Rust core. FeatureSelectionSupport и небольшой fitted
Transformer в Scala; Spark ML/stat API, без RDD. Metadata ограничена 1000
признаками до collection; полные таблицы не collect. GUI строит поля из NodeSpec.

## §6. Ограничения

В downstream estimator задайте features_column=selected_features. Внутреннее
кодирование target для χ² не заменяет исходный label в таблице: estimator
требует свой numeric label; для строкового target подготовьте его отдельно.

Примеры: feature_selection.dcraft показывает отчёт; feature_selection_pipeline.dcraft
по умолчанию показывает отбор, Preview UUID ...000006 — test vector с unseen category.
random_forest_pipeline.dcraft показывает importance; Preview ...000008 — test vectors,
Classification Metrics ...000011 — реальный downstream classifier на selected_features.

```sh
cargo run -p datacraft-cli -- run examples/pipelines/random_forest_pipeline.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/random_forest_pipeline.dcraft --output 00000000-0000-0000-0000-000000000011
cargo run -p datacraft-desktop -- examples/pipelines/random_forest_pipeline.dcraft
```

verified 2026-11-08 via pinned Spark 4.0.1
[ChiSquareTest](https://spark.apache.org/docs/4.0.1/api/java/org/apache/spark/ml/stat/ChiSquareTest.html),
[QuantileDiscretizer](https://spark.apache.org/docs/4.0.1/api/java/org/apache/spark/ml/feature/QuantileDiscretizer.html)
и фактические compiler/runtime tests. До запуска после изменения worker выполните
в workers/spark `sbt test writeClasspath`.

χ² предполагает categorical contingency table; малые expected counts могут
делать асимптотический p-value непригодным. P-values — unadjusted, не гарантия
причинности/обобщения и не автоматическая проверка гипотез с multiple testing.
Category indices не заменяют one-hot для линейных моделей. Quantile error
контролирует ранги bins, не погрешность p-value. Словари, quantiles, forest и
contingency aggregation могут потребовать значительную память/shuffle.
RF importance смещена для некоторых распределений/кардинальностей; equivalence
legacy pipelines и побитовость между ОС/версиями не заявляются.

## §7. Независимые ожидания

Три класса с двумя train rows на класс; predictor=target даёт χ²=12, df=4,
p=7*exp(-6). Independent predictor с равномерными counts даёт statistic=0,p=1.
ASSUMPTION: absolute 1e-12 для этих χ²/p чисел — provisional acceptance,
не доказанный error bound Spark/Breeze. Оракул — аналитическая contingency table,
не snapshot Spark. Selected indices/флаги и вектор известного train-only
кодирования проверяются точно. RF с одним signal и constant nuisance,
bootstrap=false/all/depth=1 даёт importance 1/0 и выбирает signal.
Изменение только test target/features не должно повторять fit/менять selection.

## §8. Численный контракт

Статистика/значимости вычисляются Spark в Double. При df=0 значимость не
выдумывается; feature не участвует в top-k. Равные scores упорядочиваются по
индексу; вектор сортируется по original index. Нет сертификации solver или
общего ULP bound. Результаты команд и неисполненные проверки — VALIDATION.md.
