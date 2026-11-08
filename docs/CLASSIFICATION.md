# Классификация и оценка предсказаний

## §1. Задача

Четыре classifier-ноды обучаются только на train и передают runtime Model
в общий Predict. Classification Metrics оценивает явно поданную test-таблицу.
Регрессионные ноды, preprocessing и проекты v1 сохраняют своё поведение.

## §2. Алгоритм

Spark ML LogisticRegression(binomial), DecisionTreeClassifier,
RandomForestClassifier, LinearSVC. Transform применяет fitted preprocessing,
Predict — fitted regression/classification. Metrics группирует пары
(label,prediction) и собирает не более 1001 count rows; более 1000 занятых
ячеек confusion matrix отклоняется. Полные датасеты не собираются на клиент.
Accuracy=sum(TP)/n; precision/recall/F1 считаются по каждому классу и усредняются
с весом support/n. При нулевом знаменателе соответствующая precision/recall/F1=0.
Взвешенные числители суммируются до деления на n; weighted recall совпадает
с accuracy для single-label classification. F1 класса=2*TP/(support+predicted).

## §3. Применения

Source → train/test → fitted preprocessing → classifier.model.
Test + preprocessing.model → Transform → Predict → Classification Metrics
или Select → Table Preview. Метрики не доказывают независимость набора:
пользователь явно подключает test. Классы кодируются одинаково на train/test;
при строковых target используйте один fitted RFormula, не отдельные indexers.

## §4. Контракт

kind: logistic_regression, decision_tree_classifier, random_forest_classifier,
linear_svc, classification_metrics. Estimators: table→model, не выход проекта.
Metrics: table→выход проекта. Общие label_column и features_column=features
различны, features — Spark ML Vector. Train labels — конечные целые 0..K-1,
2<=K<=1000, без пропусков. Logistic/SVC требуют ровно классы 0/1.
Значения null/NaN/Infinity/fractional labels не удаляются и не округляются.
Зарезервированы prediction/probability/rawPrediction; перезапись запрещена.

Logistic: fit_intercept=true, max_iter=100, reg_param=0, elastic_net_param=0,
threshold=0.5 (probability class 1, диапазон 0..1), family=binomial фиксирован.
SVC: fit_intercept=true, max_iter=100, reg_param=0, threshold=0 (finite raw margin).
Это современная L2 LinearSVC, не эквивалент старой SVMWithSGD с L1.
Tree: max_depth=5 (0..30), max_bins=32 (>=2), min_instances_per_node=1,
seed=42 (u32), impurity=gini (gini/entropy). Forest дополнительно num_trees=20,
bootstrap=true, feature_subset_strategy=auto (auto/all/sqrt/log2/onethird).
Subsampling rate=1.0; прочие параметры и convergence tol — Spark defaults.

Metrics: label_column обязателен, prediction_column=prediction, имена различны.
Обе колонки numeric с конечными неотрицательными целыми int32 class IDs.
Evaluation не требует присутствия всех train classes; неизвестный test class
участвует в ошибках. Пустой набор и invalid values — recoverable error.
Результат Classification metrics: accuracy, weighted_precision, weighted_recall,
weighted_f1. ROC/AUC, binary positive-class metrics и export matrix пока отсутствуют.

## §5. Реализация

NodeKind/NodeSpec в core; Scala ClassificationSupport отдельно от Worker protocol.
Session/GUI/CLI используют те же graph contracts; все модели живут только в job.
Spark 4.0.1 API verified 2026-11-08 via официальные
[LogisticRegression](https://spark.apache.org/docs/4.0.1/api/java/org/apache/spark/ml/classification/LogisticRegression.html),
[LinearSVC](https://spark.apache.org/docs/4.0.1/api/java/org/apache/spark/ml/classification/LinearSVC.html).
Результаты реальной компиляции/исполнения фиксируются в VALIDATION.md.

Проекты examples/pipelines: logistic_regression.dcraft, linear_svc.dcraft,
decision_tree_classifier.dcraft, random_forest_classifier.dcraft. Designated
output — Classification Metrics; Preview UUID ...000009 показывает test labels
и predictions. Примеры намеренно простые, perfect accuracy не обещает качества
на произвольных данных. Forest fixture bootstrap=false/all/3 trees/depth=1
имеет точный oracle; defaults bootstrap=true/auto/20 trees/depth=5 сохранены.

```sh
cargo run -p datacraft-cli -- run examples/pipelines/logistic_regression.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/linear_svc.dcraft --output 00000000-0000-0000-0000-000000000009
cargo run -p datacraft-desktop -- examples/pipelines/logistic_regression.dcraft
```

После изменения worker сначала выполните `sbt test writeClasspath` в workers/spark.

## §6. Ограничения

Training/distinct/groupBy могут требовать значительную RAM и shuffle; bounded
driver collection не означает общего лимита памяти. Count aggregation и seed
не обещают побитовость между ОС/версиями Spark. Модели не сохраняются; tuning,
multinomial logistic и старый L1 SVM пока не реализованы. Лимит 1000 matrix cells
ограничивает текущую оценку большого количества классов.

## §7. Независимые ожидания

Симметричный train: x=-4,-3,-2,-1 → 0; x=1,2,3,4 → 1.
Test x=-2.5,-0.5,0.5,2.5 ожидает 0,0,1,1. Binary examples задают reg_param=0.1,
далёкие от decision boundary точки: проверяются exact class IDs, не коэффициенты
или придуманный solver tolerance. Tree/Forest с bootstrap=false/all/depth=1
на этих данных имеют те же exact leaves. Defaults bootstrap=true отдельно проверяются.
Labels=[0,0,1,1], predictions=[0,1,0,1]: все четыре метрики точно 0.5.
Полная правильная классификация трёх классов даёт все метрики 1.
Изменение только test labels должно менять метрики, не predictions.

## §8. Численный контракт

Class IDs в int32 точно представимы как Double. Confusion counts — Spark Long;
отношения и weighted sums вычисляются в Double без обещания общего ULP bound.
Тесты используют точно представимые половины/единицы и проверяют schema отдельно.
Для probability проверяются размер и диапазон, не точность optimizer или
калибровка. Старые regression solver thresholds не меняются.
Независимое review и проверка новых нод на всех ОС фиксируются отдельно.
