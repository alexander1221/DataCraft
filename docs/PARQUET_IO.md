# Parquet: локальные файлы и HDFS

## §1. Задача

Сохранить рабочий Parquet Source. Добавить Local Parquet Export, Import HDFS
Parquet и HDFS Export Parquet с обычными Table-портами и без фиктивного
табличного выхода у export. Схема, nulls и metadata должны проходить round-trip.

## §2. Алгоритм

Parquet Source использует существующий Spark reader. HDFS import потоково
скачивает отдельный файл на временный локальный диск, затем использует тот
же reader. Экспорт пишет один настоящий Parquet через имеющийся ParquetWriter
и Spark ParquetWriteSupport, поток InternalRows и LocalOutputFile. Это позволяет
избежать зависимости Windows-записи от winutils.exe и сохранить Spark metadata.
Локальная публикация и HDFS upload используют temporary file и явный overwrite.

## §3. Применения

CSV/преобразования → Parquet file → Parquet Source → Preview; тот же round-trip
через HDFS, сохранение Decimal/date/timestamp/nested types/ML metadata; корректное
отклонение конфликтов назначения без повреждения существующего файла.

## §4. Контракт

Новые kind: parquet_export, hdfs_import_parquet, hdfs_export_parquet. Source
выдаёт один Table, export принимает один Table и не выдаёт портов. Local path
разрешается относительно .dcraft; HDFS path остаётся абсолютным remote path.
HDFS использует общий клиент/профили/browser Text-нод. Экспорт поддерживает
compression uncompressed/snappy/gzip/zstd, row_group_mb и явные overwrite/
create_parents. По умолчанию overwrite запрещён. Никаких CSV delimiters или
encoding для Parquet: schema и metadata берутся из бинарного файла.

HDFS import в этом пакете принимает один файл; partitioned HDFS directories
не заявляются. Существующий локальный Source сохраняет поддержку Spark parquet
directories. Временное хранение требует локального диска. Использование Spark
ParquetWriteSupport привязано к проверенной версии 4.0.1 и проверяется тестами
при обновлении Spark.

## §5. Реализация

NodeSpec, NodeKind, Project/SaveAs и инспектор используют общие контракты.
Destination Browse открывает native save picker; HDFS — общий remote browser.
Выбор не пишет файл и остаётся буфером до Apply. Export не создаёт таблицу.

ParquetIO передаёт скопированные InternalRows в ParquetWriter. Проверены
Decimal(38,10), int64, date, timestamp с microseconds, массивы, maps и VectorUDT
с metadata. Spark reader может расширять nullable=true; NOT NULL flags не
гарантируются. Protocol JSON передаёт raw Spark rows/schema, serde_json использует
arbitrary_precision: Decimal/int64 не округляются через ujson Double.
Численные параметры алгоритмов по-прежнему f64.

## §6. Ограничения

Экспорт однопоточный на driver и не создаёт partitioned dataset. toLocalIterator
может держать крупнейшую partition; row-group buffers тоже требуют RAM.
HDFS требует диска под полный binary spool. Универсальная bounded-RAM обработка
не гарантируется. При hard kill могут остаться OS temporary files.
Local overwrite требует atomic move на файловой системе назначения; неподдерживаемая
операция возвращает ошибку без небезопасного fallback. Внешние writers не
координируются. Timestamp — TIMESTAMP_MICROS. Encryption и schema evolution
между несколькими HDFS-файлами не реализованы.

## §7. Проверяемые примеры

Из корня после сборки worker:

```text
cargo run -p datacraft-cli -- run examples/pipelines/parquet_export.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/parquet_import.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/hdfs_export_parquet.dcraft
cargo run -p datacraft-cli -- run examples/pipelines/hdfs_import_parquet.dcraft
```

Сначала export, затем import; повторный export по умолчанию отклоняется.
HDFS требует stand из examples/hdfs. Tests используют уникальные пути.

## §8′. Устойчивость и проверки

ParquetIOSuite проверяет PAR1/footer/кодеки и независимые точные значения,
пустую таблицу, защиту существующего файла и wire precision. ParquetHdfsSuite
использует реальный NameNode/DataNode, проверяет overwrite, missing/corrupt
files и сохранность назначения при ошибке. GUI acceptance нажимает Run project
и сравнивает строки локального/HDFS round-trip. Фактические результаты —
в VALIDATION.md; подготовленный workflow не означает удалённый PASS.
