# Архитектура DataCraft

## HDFS и файловый экспорт

HdfsClient в Scala использует Hadoop WebHdfsFileSystem с DataNode redirects.
DelimitedText/HdfsText выполняют потоковый parsing и текстовую сериализацию;
ParquetIO — ParquetWriter и Spark ParquetWriteSupport 4.0.1. Новых runtime
dependencies нет. Локальный export пишет один файл через JDK NIO.
HDFS imports используют локальный spool до завершения query/cache; worker
удаляет его при инвалидации. Hard kill может оставить OS temporary file.
UI не делает HTTP-запросы: browser — фоновая команда Session.

Экспорт имеет side effect: undo графа не отменяет файл. Перезапись по умолчанию
запрещена. Публикация выполняется rename после закрытия temporary file;
существующее назначение заранее не удаляется. Remote access — явная опция
HDFS-нод. Обычные local pipelines не требуют сети. Secrets и Kerberos отсутствуют.

## Компоненты

```text
apps/datacraft-cli          clap, команды и отображение результатов
apps/datacraft-desktop      eframe/egui, canvas, Session adapter, preview
crates/datacraft-core       сериализуемый DAG, UUID, NodeSpec, порты, валидация
crates/datacraft-project    .dcraft, версии, сохранение, разрешение путей
crates/datacraft-engine     Session, CommandSpec, undo/redo, JobManager, JVM worker
xtask                      архитектурная политика, реестры, Spark fixtures, CI
workers/spark               Scala + Spark SQL / ML
examples/data              небольшие CSV и NDJSON fixtures
examples/pipelines         подготовленные проекты
examples/expected          независимые ожидаемые схемы и значения
```

Границы Rust crates проверяет `cargo xtask layers` по Cargo metadata; `commands`
и `nodes` проверяют живые реестры, `fixtures` исполняет примеры через Session.
Политика и ограничения: [ARCHITECTURE_CHECKS.md](ARCHITECTURE_CHECKS.md).

Первый UI-клиент находится в отдельном crate слоя ui. DataFusion пока отсутствует.
Rust core не зависит от Spark или egui.
Для первой вертикали выбрана Spark 4.0.1 с Scala 2.13.16 и Java 17/21:
[требования Spark](https://spark.apache.org/docs/4.0.1/).
Это фиксированная исходная цель совместимости, а не утверждение о последней версии.

Проверены публичные описания [ArtCraft](https://getartcraft.com/) и
[MaterialCraft](https://github.com/storytold/materialcraft): ориентиры — нативное
приложение, локальное исполнение и независимый headless-интерфейс. Их код
и ассеты в эту реализацию не переносились.

## Постоянная модель

Отбор признаков возвращает fitted preprocessing Model для Transform:
statistical top-k хранит train-only dictionary/indices, RF selection — slicer.
RF внутри selection-ноды не подменяет downstream Predictor. Все три selection
ноды имеют выбираемый report output. Контракт: [FEATURE_SELECTION.md](FEATURE_SELECTION.md).

Подготовка признаков: Source → Split.train → fitted preprocessing;
Split.test + preprocessing.model → Transform → Preview/Predict.
Worker хранит таблицы по `(node UUID, port ID)`: train и test не заменяют друг
друга. Fitted preprocessing, regression и classification models находятся
в runtime-only map одного запроса, JSON содержит только конфигурации.
Assemble Features объединяет числовые колонки и ML-векторы; промежуточные
DataFrame остаются lazy. Контракт: [PREPROCESSING.md](PREPROCESSING.md).

Проекты версии 1 допускают черновой граф. Load/save проверяют целостность,
а не готовность к исполнению. Строгие Project::validate/Graph::validate
сохраняют прежний смысл; Graph::diagnostics возвращает проблемы параметров
и входов. Перед запуском проверяется готовность выбранного выхода и его
предков. Детальный контракт: [PROJECT_FORMAT.md](PROJECT_FORMAT.md).

Встроенные ноды имеют единый декларативный NodeSpec в core: category, порты
с cardinality, параметры, defaults и ограничения. Graph читает metadata напрямую;
NodeKind сохраняет типизированную JSON-конфигурацию. CLI nodes/--json использует
тот же реестр, не запускает Spark. Details: [NODES.md](NODES.md).
Scala worker остаётся отдельной реализацией backend и не импортирует Rust metadata;
проверки протокола на границе процессов сохраняются.

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

CLI открывает проект через Session::dispatch(project.open), а выполнение
передаёт execution.run. Session хранит Project за immutable API, current path,
saved checkpoint и ограниченную историю. Runtime config, Worker и результаты
отделены от snapshot Project и не сериализуются в .dcraft. Desktop хранит выбор,
pan/zoom, незавершённые drag/connection и буфер параметров отдельно. Постоянные
правки Add/Remove/Configure/Connect/Disconnect и завершённый Move отправляются
в Session. Палитра и поля инспектора читают NodeSpec. Подробнее:
[DESKTOP.md](DESKTOP.md), [GRAPH_EDITOR.md](GRAPH_EDITOR.md).
Системные диалоги rfd и буфер Rename относятся только к UI layer. Native future
опрашивается без блокирующего ожидания, результаты проверяются по revision/UUID.
Rename использует существующий Command::Rename. Контракт: [FILE_DIALOGS.md](FILE_DIALOGS.md).
Точные правила атомарности и SaveAs: [COMMANDS.md](COMMANDS.md).

Полный граф проходит проверку целостности; параметры и обязательные входы
проверяются только для выбранной ветки. Исполняется только замыкание
зависимостей выбранного выхода. Выход задаётся флагом CLI, затем `output_node`,
затем единственным кандидатом `TablePreview` / `LinearRegression` /
`RegressionMetrics` / `ClassificationMetrics`.

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

Есть отдельный session-local кэш поузлового режима; нет сохранения моделей
между открытиями проекта и процентного прогресса Spark.
GUI event loop опрашивает Session и не вызывает блокирующий Worker::run.
Обычный Run создаёт свой JVM и ленивый план. RunNode/RunDownstream используют
постоянный worker и materialized DataFrame/ML objects. Кэш сбрасывается по
зависимостям и не входит в Project/undo. Контракт: [NODE_EXECUTION.md](NODE_EXECUTION.md).
Результаты помечаются project_revision; это проверка
снимка графа, а не обнаружение внешних изменений dataset или runtime settings.
Низкоуровневый Worker::run блокирует runner thread; Session::dispatch(Run)
возвращает Job ID. Supervisor отдельно обрабатывает cancel/deadline, не захватывая
reader stdout. Терминальный статус означает завершение cleanup. Контракт:
[EXECUTION.md](EXECUTION.md). Процент выполнения Spark и Ctrl+C CLI пока отсутствуют.
Spark SQL и ML могут требовать значительную память независимо от лимита preview.
Заявления об универсальном out-of-core исполнении отсутствуют.
