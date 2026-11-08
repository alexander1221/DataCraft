# Проверки архитектуры и fixtures

## §1. Задача

Воспроизводимые команды проверяют границы Cargo crates, реальные реестры
и выполнение эталонных проектов. Ненулевой exit code означает нарушение
контракта, ошибку инструмента или missing prerequisite; no-op checks недопустимы.

## §2. Алгоритм

layers читает cargo metadata --format-version 1 --locked --all-features.
Workspace crates обязаны объявлять package.metadata.datacraft.layer.
Рёбра берутся из resolve.nodes.deps по package ID, а не alias name. Все
normal/dev/build и target-specific зависимости учитываются без фильтрации ОС.
Проверяются direct workspace edges, допустимое транзитивное замыкание слоёв,
запрещённые внешние package families и циклы workspace. Ошибки сортируются.

commands проверяет CommandSpec и запускает существующий независимый contract
test enum/serde/registry. nodes проверяет NodeSpec и typed draft factories:
kind, полный набор сериализуемых параметров и defaults должны совпадать.
fixtures открывает проекты через Session, запускает настоящие jobs и сравнивает
имена таблиц, поля/типы схем, числа строк и значения с отдельным JSON oracle.

## §3. Применения

```sh
cargo xtask layers
cargo xtask commands
cargo xtask nodes
cargo xtask fixtures
cargo xtask ci
cargo xtask ci --with-spark
```

ci без --with-spark выполняет Rust fmt/check/test/Clippy и три архитектурных
проверки. Он явно сообщает, что Spark пропущен. --with-spark дополнительно
запускает sbt test writeClasspath, fixtures и пример отмены/восстановления JVM.

## §4. Контракт политики

Текущие слои: core, project, engine, cli, ui, tooling. Дополнительно определены
types/graph/nodes/planner/runtime/connectors/mcp для будущих реальных crates.
Пустые crates не создаются. Неизвестный/неуказанный layer — ошибка.

| Layer | Разрешённые прямые workspace dependencies |
|---|---|
| types | types |
| core | core, types |
| graph | graph, types |
| nodes | nodes, core, types, graph |
| project | project, core, types, graph, nodes |
| planner | planner, core, types, graph, nodes, project |
| connectors | connectors, core, types, graph, nodes |
| runtime | runtime, core, types, graph, nodes, planner, connectors |
| engine | engine, core, types, graph, nodes, project, planner, runtime, connectors |
| cli | cli, core, types, graph, nodes, project, engine |
| mcp | mcp, core, types, graph, nodes, engine |
| ui | ui, core, types, graph, nodes, engine |
| tooling | все слои |

Production crates не зависят от tooling. Самослой разрешён для будущего
разделения на несколько crates, но циклы запрещены. CLI → project оставлен
намеренно: Inspect строит начальный временный Project; open/execution проходят
через Session. UI/MCP не имеют прямого runtime доступа; транзитивный runtime
через engine допустим.

Для core/types/graph/nodes/project запрещена транзитивная DataFusion-family.
Для всех headless layers и cli/mcp запрещены транзитивные UI families:
egui/egui-*, eframe, winit, iced/iced_*, druid, tauri/tauri-*, slint/i-slint-*, rfd.
ui не может непосредственно зависеть от DataFusion. Tooling освобождён от этих
external bans, но его нельзя включать в production closure.
Build/dev зависимости не получают исключения. Циклы проверяются по всем
workspace edges, включая dev/build. Это политика проекта, а не описание Cargo.

--offline ограничивает Cargo, не сеть sbt. --java и --worker-classpath имеют
те же DATACRAFT_* env overrides, что CLI. --timeout-seconds: default 300.
Нужен заранее созданный workers/spark/target/classpath.txt либо ci --with-spark.
Нет автоматического SKIP при отсутствии Java/worker.

## §5. Реализация

xtask — developer crate, использующий реальные реестры/Session. .cargo/config.toml
задаёт alias. Metadata DTO содержит только используемые поля. Отсутствующая
resolve graph/нода и неизвестный dep kind отклоняются. Подкоманды запускаются
от workspace root, без конкатенации shell-команд с пользовательскими путями.

## §6. Замечания

layers не анализирует Scala imports, произвольные команды build.rs, dynamic
libraries и Rust private/public API. Обход Session требует отдельного review.
Будущие UI families нужно явно дополнять; правило не является универсальным
классификатором пакетов. Metadata all-features может требовать registry/cache.

## §7. Независимая приёмка

Тесты layers используют metadata graphs с известными нарушениями: renamed,
transitive, target/dev/build UI edges, DataFusion, неизвестный слой, cycles
и допустимый UI → engine → runtime. Oracle fixtures написан по формуле
amount=2*quantity+3 и заранее заданным filter/limit, не из вывода DataCraft.
Comparator обязан отклонять неправильные значения, схемы, число строк, NaN
и ошибочный manifest. Реальные результаты команд: VALIDATION.md.

## §8′. Границы и численный контракт

CSV/draft строки и целочисленные поля сравниваются точно. Для ML oracle
использует прежний acceptance threshold WorkerSuite: absolute tolerance 1e-6.
ASSUMPTION: эта величина не выведена из анализа Spark solver. fixtures печатает
предупреждение и проверяет provisional acceptance contract, а не numerical
equivalence. Ошибки чисел должны быть конечными, неизвестные поля oracle
отклоняются. Допуск не расширяется по результату прогона. Порядок строк
проверяется только для этих маленьких fixtures с одним локальным source file,
не для произвольного Spark DataFrame.

Отмена/timeout являются ошибкой fixtures. Успешный spawn/exit code сам по себе
не доказывает корректность pipeline. Fixtures охватывают sample/draft/regression/
prediction; Parquet source дополнительно проверяется Scala WorkerSuite.
