# Архитектура DataCraft

## Компоненты

```text
apps/datacraft-cli          clap, команды и отображение результатов
crates/datacraft-core       сериализуемый DAG, UUID, порты, валидация
crates/datacraft-project    .dcraft, версии, сохранение, разрешение путей
crates/datacraft-engine     lifecycle JVM и клиент протокола
workers/spark               Scala + Spark SQL / ML
examples/data              небольшой CSV
examples/pipelines         подготовленные проекты
```

UI и DataFusion не входят в текущий этап. Rust core не зависит от Spark.
Для первой вертикали выбрана Spark 4.0.1 с Scala 2.13.16 и Java 17/21:
[требования Spark](https://spark.apache.org/docs/4.0.1/).
Это фиксированная исходная цель совместимости, а не утверждение о последней версии.

Проверены публичные описания [ArtCraft](https://getartcraft.com/) и
[MaterialCraft](https://github.com/storytold/materialcraft): ориентиры — нативное
приложение, локальное исполнение и независимый headless-интерфейс. Их код
и ассеты в эту реализацию не переносились.

## Постоянная модель

`.dcraft` содержит `format_version`, `name`, `nodes`, `edges`, `output_node`.
Нода содержит UUID `id`, `position: [x, y]`, `config` с тегом `kind`.
Ребро содержит `from`, `from_port`, `to`, `to_port`; идентификаторы портов
в первой версии — `table` и `model`.

UUID сохраняются независимо от будущих UI-индексов. Перед записью ноды и рёбра
сортируются; топологическая сортировка выбирает доступные UUID по возрастанию.
Неизвестная версия отклоняется до интерпретации содержимого. Сохранение через
временный файл в том же каталоге защищает существующий проект от частичной
записи; несовместимый или повреждённый существующий файл не перезаписывается.
Относительные пути преобразуются только в runtime-копии.

## Исполнение

Полный граф проходит структурную проверку. Исполняется только замыкание
зависимостей выбранного выхода. Выход задаётся флагом CLI, затем `output_node`,
затем единственным кандидатом `TablePreview` / `LinearRegression`.

Rust запускает `java` напрямую, без shell. stdin/stdout передают JSON Lines;
stderr содержит диагностику Spark. Worker использует одну SparkSession
`local[2]`, без UI Spark, с адресами driver на loopback.
DataFrame остаются ленивыми до preview или обучения. Нода Linear Regression
выдаёт `Model`, Predict принимает `Table` и `Model`.
Отчёты выбранной модели возвращаются как именованные таблицы результатов.

ML использует DataFrame API `spark.ml`, без перехода к RDD:
[LinearRegression API](https://spark.apache.org/docs/4.0.1/api/scala/org/apache/spark/ml/regression/LinearRegression.html).
Первая Assemble Features принимает только числовые колонки; пропуски являются
ошибкой, а не поводом автоматически удалить строки.

## Протокол версии 1

Запрос: `protocol_version`, UUID `request_id`, `operation: "run"`, `project`,
`output_node`, `execution_order` (UUID предков и самого выхода).
Worker ожидает граф, проверенный Rust-клиентом; это внутренний протокол.

Ответ: `protocol_version`, `request_id`, `ok`, `tables` при успехе;
`node`, `message` при ошибке. Таблица: `name`, Spark `StructType` JSON в `schema`,
JSON-объекты в `rows`. Неопределённые числовые метрики передаются как `null`.
Лимит preview применяется до `collect`; клиент ограничивает размер одного frame.
Ошибка ноды не завершает SparkSession: следующий запрос может быть исполнен.

## Границы первой версии

Нет кэша между запросами, сохранения моделей, прогресса или GUI event loop.
`Worker::run` блокирует вызывающий поток; будущий desktop должен использовать
background thread. Метод `cancel` завершает процесс, но текущий API не даёт
вызывать его конкурентно с `run`; полноценный job handle ещё предстоит сделать.
Spark SQL и ML могут требовать значительную память независимо от лимита preview.
Заявления об универсальном out-of-core исполнении отсутствуют.
