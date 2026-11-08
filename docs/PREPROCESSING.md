# Подготовка данных и признаков

## §1. Задача

Локальные JSON/CSV/Parquet данные разделяются на train/test до обучения признаков.
Index Categoricals, Reference Code Categoricals и Make Design Matrix обучаются
только на table-входе ноды; возвращают table и fitted model. Transform применяет
готовую модель к другой таблице без fit. Регрессионный Predict остаётся отдельным.

## §2. Алгоритм

Train/Test Split использует стабильные key_columns и seed, Spark xxhash64 и pmod:
score = pmod(xxhash64(seed как Long, keys...), 2^53), cutoff=floor(train_fraction*2^53).
Train содержит score<cutoff, test — дополнение. Нет сортировки, cache или collect.
Одинаковые ключи (включая null) остаются вместе. Порядок/партиции не участвуют
в hash; типы и значения ключей участвуют. Доля приблизительная, не точное число строк.

Категории индексируются StringIndexer с alphabetAsc, затем при необходимости
OneHotEncoder. fit получает только train; Transform использует сохранённый Transformer.
Design Matrix использует Spark RFormula. Assemble Features принимает числа и
ML-векторы, поэтому результаты кодирования можно собрать для Linear Regression.

## §3. Применения

Source → Split.train → Fit preprocessing → Linear Regression.
Split.test + fitted preprocessing.model → Transform → Predict с regression.model
→ Table Preview. Preview любой train/test ветки исполняется отдельно по UUID выхода.
Демонстрационные проекты и реальные результаты перечислены в VALIDATION.md.

| Проект в examples/pipelines | Проверяемый сценарий |
|---|---|
| index_categoricals.dcraft | Fit на id<=4, Transform на id>=5; a→0, unseen c→2 |
| reference_code.dcraft | Те же ветки, известная a→[1,0], unseen c→[0,0]; сборка с x |
| design_matrix.dcraft | y~x-1, fit на train, label=13/15 на test |
| train_test.dcraft | Hash split 128 строк; train fit RFormula, test Transform |

Запуск: `cargo run -p datacraft-cli -- run examples/pipelines/train_test.dcraft`.
Окно: `cargo run -p datacraft-desktop -- examples/pipelines/train_test.dcraft`.
После изменения Scala-кода сначала выполните в workers/spark `sbt test writeClasspath`.
Векторы в preview пока показаны исходным Spark UDT JSON; metadata остаются в schema.
Index и Design имеют точные scalar oracles в examples/expected/pipelines.json.
Reference проверяется с декодированием sparse/dense векторов в WorkerSuite;
Split — инвариантами disjoint/exhaustive/grouping/seed/repartition в Scala-тестах.
Membership Spark hash не выдается за независимо рассчитанный точный row oracle.

## §4. Контракт

Новые kind: json_source, train_test_split, make_design_matrix, index_categoricals,
reference_code_categoricals, transform. Проект v1 и старые ноды совместимы;
старые приложения не обязаны понимать новые kind. JSON Source читает NDJSON.
Split: key_columns nonempty/unique, train_fraction finite строго (0,1), seed u32.
Выходы train/test имеют тип Table. Fit-ноды: вход table, выходы table/Model.
Transform: входы table/model, выход table; не принимает regression model.

Index: columns nonempty/unique, keep_unknown=true по умолчанию; выходы c_idx.
Reference Code: columns, keep_unknown=false, drop_last=true; c_idx и c_idx_vec.
При keep_unknown=true StringIndexer добавляет bucket неизвестных; с drop_last=true
нулевой код соответствует этому bucket. Это не обещание полного ранга design matrix.
При keep_unknown=false неизвестное значение даёт recoverable error. Из-за lazy
execution ошибка значения может быть отнесена к Table Preview, где вычислен план.
Design Matrix: formula, features_column=features, label_column=label,
keep_unknown=false. Имена выходов различны; перезапись посторонних колонок запрещена.
String order — alphabetAsc. Числовые response и метаданные вектора сохраняются.
RFormula обрабатывает формулу, Rust проверяет только её непустоту и имена выходов.

## §5. Реализация

NodeKind/NodeSpec в core; порт-адресуемые DataFrame в Worker и fitted Transformer
в runtime-only map. UI генерирует поля из реестра, без отдельных схем конфигурации.
Официальные API Spark 4.0.1:
[RFormula](https://spark.apache.org/docs/4.0.1/api/java/org/apache/spark/ml/feature/RFormula.html),
[OneHotEncoder](https://spark.apache.org/docs/4.0.1/api/java/org/apache/spark/ml/feature/OneHotEncoder.html).

## §6. Замечания

Split — grouped hash split, не randomSplit по экземплярам строк. Для близкой доли
нужно достаточно разных ключей; train/test могут оказаться пустыми. Выбирайте
ключ по сущности, между которой не должно быть утечки. Key schema, seed, Spark
version и исходные значения нужны для воспроизведения; эквивалентность старым
нодам не заявляется. Fit категорий/RFormula может требовать значительную память
для словаря/плана; ограничение preview не ограничивает память fit.
Модели живут только внутри одного job и не записываются в .dcraft.

## §7. Приёмка

Независимые инварианты: train/test не пересекаются и дают исходный набор ключей;
перестановка/репартиционирование не меняют membership; одинаковые ключи вместе.
Для обучения используются известные категории a/b, в test добавляется unseen:
его индекс/вектор не должен становиться новой обученной категорией. Числовая
формула y~x+z проверяется по заранее заданным значениям и порядку признаков.
Проверяются schema/metadata, null/unknown, collisions, malformed formula,
неверная модель, пустой train и старые CSV/ML pipelines.

## §8. Численный контракт

Cutoff: умножение конечного f64 из (0,1) на 2^53 не переполняется и является
точным двоичным масштабированием; преобразование положительного результата
в Long отбрасывает дробную часть. Поэтому квантование доли меньше 2^-53.
Это граница квантования, не статистическая гарантия размера выборки и не
численная эквивалентность Spark randomSplit. Категориальные индексы, 0/1 и
выбранные малые целочисленные design-примеры сравниваются точно.
Предыдущий ML threshold 1e-6 остаётся ASSUMPTION для старых regression fixtures,
новые допуски под вывод solver не подгоняются.
