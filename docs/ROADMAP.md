# Дальнейшая разработка

## HDFS/Parquet — 2026-11-12

Реализованы HDFS Text import/export, remote browser, локальный Parquet Export
и HDFS Parquet import/export. Всего 32 типа. Контракты — HDFS_TEXT.md/PARQUET_IO.md;
фактические проверки — VALIDATION.md. Следующие задачи: ручная приёмка HDFS-форм,
partitioned Parquet datasets, затем сохранение/загрузка fitted моделей.
TLS/Kerberos требуют отдельного стенда и текущим simple-auth Hadoop не проверены.

## Текущий фундамент

Поузловой запуск с постоянным worker, materialized session cache, инвалидацией
по зависимостям и статусами на canvas реализован. Межсессионный кэш и
надёжные checkpoints на диске пока не реализованы: [NODE_EXECUTION.md](NODE_EXECUTION.md).

NodeSpec, черновики, Session с командами/undo, фоновые jobs, отмена/timeout
и xtask реализованы. Desktop создаёт проекты, добавляет/удаляет ноды, изменяет
параметры и связи через Session, сохраняет граф и предоставляет Run/Cancel/preview.
Добавлены системные Open/Save as, Browse CSV/Parquet/JSON и Rename с Undo/Redo;
ручной ввод путей сохранён. Контракты: DESKTOP.md, GRAPH_EDITOR.md, FILE_DIALOGS.md;
фактические результаты проверок — VALIDATION.md.

## Подготовка данных: реализованный пакет

Добавлены NDJSON Source, grouped Train/Test Split с seed и key_columns,
Make Design Matrix (RFormula), Index Categoricals, Reference Code Categoricals
и Transform. Fit получает train, Transform применяет модель к test без fit.
Assemble Features принимает числовые колонки и ML-векторы. Контракт, примеры
и ограничения: [PREPROCESSING.md](PREPROCESSING.md). Полный исходный список
Spark-нод пока не перенесён.
Настройки source пока фиксированы: JSON — NDJSON, CSV — header/infer_schema.

## Регрессия: реализованный пакет

GLM (gaussian/binomial/poisson/gamma), Decision Tree Regressor и Random Forest
Regressor отдают fitted Model в общий Predict. Regression Metrics рассчитывает
RMSE/MSE/MAE/R² на явно поданном наборе. Три новых примера обучаются на train,
оцениваются на test; старый отчёт training metrics Linear Regression сохранён.
Контракт и ограничения: [REGRESSION.md](REGRESSION.md).

## Классификация: реализованный пакет

Logistic Regression и Linear SVM (LinearSVC, L2, binary 0/1), Decision Tree
Classifier и Random Forest Classifier (также multiclass), общий Predict и
Classification Metrics. Четыре примера обучаются на train и оцениваются на test.
Метрики: accuracy, weighted precision/recall/F1.
Контракты и ограничения: [CLASSIFICATION.md](CLASSIFICATION.md).
Исходная L1 SVMWithSGD не переносилась дословно; эквивалентность не заявляется.

## Отбор признаков: реализованный пакет

Feature Selection возвращает bounded χ²-отчёт; Feature Selection Pipeline
выбирает top-k на train и применяет fitted coding к test без target.
Random Forest Pipeline отбирает компоненты ML-вектора по classification/regression
importance. Отчёты доступны как outputs, обе pipeline-модели — через Transform.
Всего 27 типов нод. Контракт: [FEATURE_SELECTION.md](FEATURE_SELECTION.md).
Статистические допущения и ограничения не означают эквивалентность legacy нодам.

## Следующие пакеты Spark-нод

1. **Модели и совместимость:** сохранение/загрузка модели вместе с preprocessing,
   воспроизводимые параметры/схемы и проверка совместимости примеров.
2. **Статистическая приёмка:** multiple-testing correction, информативные
   diagnostics sparse contingency tables и отдельная оценка на реальных данных.

Каждый пакет включает работающий backend, NodeSpec, параметры/порты GUI,
пример pipeline и независимые проверки результатов. Не добавлять ноды-заглушки.
Полная эквивалентность исходным нодам не считается доказанной только из-за
совпадения названий. Срок полного переноса пока не оценён.

Полировка GUI не блокирует следующие пакеты. Отдельно остаются ручная
приёмка диалогов на всех целевых ОС, широкие таблицы/схемы, поиск нод,
multi-selection и drag-to-connect.

## После первой вертикали

- Явная политика перезаписи артефактов моделей и экспорта.
- Схема входов в инспекторе, отображение типов, обработка пропусков.
- Ограничение размеров отдельных значений preview и нагрузки worker.
- Проверки Windows/Linux и упаковка приложения с документированным JVM runtime.

Livy, удалённые кластеры и Spark Connect не входят в ближайший local-only этап.
Пользовательский Scala Script не переносится: исходное ограничение на выполнение
непроверенных скриптов сохраняется.
