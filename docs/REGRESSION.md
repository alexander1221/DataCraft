# Регрессия и оценка предсказаний

## §1. Задача

Добавить Generalized Linear Regression, Decision Tree Regressor и Random Forest
Regressor, сохраняя прежние Linear Regression/Predict и проекты v1. Отдельная
Regression Metrics оценивает предсказания на явно поданной таблице.

## §2. Алгоритм

Fit получает только training table и возвращает runtime Model. Predict применяет
эту модель к test features без повторного fit. Spark SQL/ML выполняет обучение;
таблицы между нодами не материализуются. Metrics агрегирует count, SSE,
MAE и population variance label; на клиент собирается только одна строка.
MSE=SSE/n, RMSE=sqrt(MSE), R²=1-MSE/variance(label).

## §3. Применения

Source → train/test → fitted preprocessing → estimator.model.
Test + preprocessing.model → Transform → Predict(estimator.model) → Regression
Metrics. Вместо Metrics можно подключить Select → Table Preview.
Нода не определяет происхождение входа автоматически: оценка на train остаётся
обучающей, а не проверкой обобщения. Примеры используют отдельные test rows.

## §4. Контракт

Новые kind: generalized_linear_regression, decision_tree_regressor,
random_forest_regressor, regression_metrics. Первые три: table→model;
выход проекта для них — Predict→Preview/Metrics. Прежний прямой отчёт Linear
Regression сохраняется. Predict принимает все четыре regression model types,
Transform — только preprocessing. Модели не сохраняются в .dcraft.

Общие параметры: label_column, features_column=features, разные имена.
GLM: family=gaussian (gaussian/binomial/poisson/gamma), link=default;
fit_intercept=true, max_iter=25, reg_param=0. Допустимые link пары:
gaussian identity/log/inverse; binomial logit/probit/cloglog;
poisson log/identity/sqrt; gamma inverse/identity/log. default выбирает первый.
Tweedie, weights, offsets, linkPower пока не поддерживаются. Convergence tol
остаётся штатным Spark default; это не допуск приёмочного теста.

Tree: max_depth=5 (0..30), max_bins=32 (>=2), min_instances_per_node=1,
seed=42 (u32), impurity=variance фиксирован. Forest дополнительно num_trees=20,
bootstrap=true, feature_subset_strategy=auto (auto/all/sqrt/log2/onethird).
Subsampling rate фиксирован 1.0; bootstrap управляет повторной выборкой.

Metrics: table→выход проекта, label_column обязателен, prediction_column=prediction;
имена различны, обе колонки numeric. Пустая таблица, null/NaN/Infinity, overflow
агрегатов — recoverable error. Для constant label R²=null, остальные метрики
определены. Нет молчаливого исключения невалидных строк.

## §5. Реализация

NodeKind/NodeSpec в core, общие команды Session, автоматически построенный
инспектор GUI. Scala RegressionSupport отделяет fit/predict/metrics от протокола.
Формат проекта и JSON-line protocol остаются v1.
API проверяются по pinned Spark 4.0.1 JavaDoc и компиляцией установленным Scala.
verified 2026-11-08 via официальные
[GeneralizedLinearRegression](https://spark.apache.org/docs/4.0.1/api/java/org/apache/spark/ml/regression/GeneralizedLinearRegression.html),
[RandomForestRegressor](https://spark.apache.org/docs/4.0.1/api/java/org/apache/spark/ml/regression/RandomForestRegressor.html)
и actual compiler/tests. Это не заявление о последней версии Spark.

Примеры: `examples/pipelines/glm_regression.dcraft`, `decision_tree.dcraft`,
`random_forest.dcraft`. Designated output — Regression Metrics. Для просмотра
предсказаний используйте Preview с UUID 00000000-0000-0000-0000-000000000009:

```sh
cargo run -p datacraft-cli -- run examples/pipelines/decision_tree.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/decision_tree.dcraft --output 00000000-0000-0000-0000-000000000009
cargo run -p datacraft-desktop -- examples/pipelines/random_forest.dcraft
```

Forest-пример намеренно bootstrap=false, num_trees=3, feature_subset_strategy=all,
depth=1 ради точного независимого oracle. Production defaults остаются
bootstrap=true, num_trees=20, auto, depth=5. На обоих сценариях есть реальные тесты.

## §6. Ограничения

Fit может требовать существенную RAM; лимит preview не ограничивает обучение.
Seed не обещает побитовое совпадение forests между ОС/версиями Spark.
Binomial GLM — прогноз среднего response; классификационные метрики/порог
здесь не вводятся. Нет сохранения моделей, tuning или независимого review.

## §7. Независимые ожидания

GLM gaussian/identity: y=2x+3, fit x=1..4, test x=5/6 даёт 13/15.
ASSUMPTION: прежний acceptance threshold 1e-6 применяется и к GLM solver;
это provisional контракт, не доказанная граница численной ошибки.
Другие family проверяются на константном отклике с математически известным
средним, тем же явно provisional threshold, а также на невалидных labels.
Tree: train (0,2),(2,2),(6,10),(8,10), depth=1; test x=1/7 даёт точно 2/10.
Forest bootstrap=false, all features и полная выборка повторяет эти leaves;
это тестируемый частный случай, не утверждение о произвольном RF.
Metrics labels=[1,3], predictions=[2,2]: SSE=2, MSE=1, RMSE=1, MAE=1,
variance=1, R²=0, все значения точно представимы. Constant-label R²=null.

## §8. Численный контракт

Metrics приводит numeric inputs к Double как Spark ML. Значения вне точного
целочисленного диапазона Double могут округляться; алгоритм не обещает ULP bound
для произвольных данных. SSE может переполняться, var_pop использует Spark
агрегацию; такие нечисловые результаты отклоняются. Предварительные допуски
solver не выводятся из наблюдаемого успешного прогона. Случаи 0/1 и leaf means
проверяются точно; будущие численные гарантии требуют отдельного анализа.
