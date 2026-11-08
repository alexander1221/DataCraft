# Session и команды

## §1. Задача и инварианты

Session — headless application facade. Project выдаётся только по неизменяемой
ссылке. Любая правка существующего проекта проходит через Command. История
содержит снимки только persistent Project; JVM и результаты в неё не попадают.
Неуспешная команда не меняет проект, revision, историю, dirty и путь сохранения.

## §2. Алгоритм

Dispatcher проверяет availability. Правка применяется к копии Project, затем
проверяется validate_structure; только после успеха копия заменяет состояние.
Предыдущий снимок помещается в ограниченную историю, redo очищается. Undo/redo
перемещают снимки между стеками. Revision возрастает монотонно, в том числе
при undo/redo. No-op не создаёт историю и не очищает redo.
Ноды и рёбра канонически сортируются на входе Session и после правок, поэтому
переподключение того же ребра не создаёт ложный dirty из-за порядка в Vec.

## §3. Применения

Редактирование черновика без GUI; атомарное удаление ноды с рёбрами; пакет правок
одним undo; сохранение и продолжение работы; общий API для CLI, GUI и будущего MCP.
Drag завершается одной Move-командой; live drag остаётся временным UI state.

## §4. Контракт

Command — сериализуемый Rust enum с устойчивым тегом command и parameters.
CommandSpec содержит id, label, mutates_project, undoable и типы параметров.
Metadata отделена от handlers. Все доступные команды исполняемы.

| ID | Действие | Undo |
|---|---|---|
| graph.node.add | добавить Node с UUID и typed config | да |
| graph.node.remove | удалить ноду, её рёбра и designated output при необходимости | да |
| graph.node.configure | заменить typed config, сохранив структурную корректность | да |
| graph.node.move | сохранить конечную позицию | да |
| graph.edge.connect | добавить совместимое ребро | да |
| graph.edge.disconnect | удалить существующее ребро | да |
| project.output.set | задать или очистить designated output | да |
| project.rename | изменить непустое имя | да |
| graph.batch | атомарный пакет только проектных правок | да, одна запись |
| history.undo / history.redo | восстановить снимок | сами не записываются |
| project.open | открыть проект; dirty требует discard_changes=true | нет |
| project.new | создать пустой проект с именем; dirty требует discard_changes=true | нет |
| project.save | сохранить по текущему пути | нет |
| project.save_as | сохранить в указанный файл | нет |
| execution.run | выполнить выбранную ветку через Spark | нет |
| execution.cancel | запросить остановку текущего Job ID | нет |
| execution.status | получить JobInfo | нет |

В реестре 18 команд. New принимает name и optional discard_changes (default false).
Пустое имя отвергается без изменения Session. New/Open запрещены при активном
job, очищают history, завершённый job и предыдущие результаты. New сбрасывает
path и saved checkpoint; до SaveAs проект dirty. Revision возрастает.

Default limit истории: 100 снимков; 0 отключает undo. Это ограничение числа
снимков, не памяти в байтах. Snapshot содержит только граф и metadata, без данных.
Factory from_project создаёт несохранённый dirty-проект. Open создаёт clean
состояние и очищает историю/старые результаты. Save отмечает checkpoint;
dirty определяется сравнением Project с сохранённым снимком, а не revision.
Undo к сохранённому содержимому сбрасывает dirty, даже при новой revision.

Batch не допускает open/save/run/undo/redo, включая вложенные batch. Его
промежуточные состояния могут быть неполными; конечное обязано быть структурно
корректным. Повторный connect и отсутствующий disconnect — ошибки.
Глубина вложенности ограничена 64; превышение — InvalidBatch.

SaveAs сохраняет смысл относительных source paths: при смене каталога пути
перебазируются относительно нового каталога, а при несовместимом корне —
становятся абсолютными. Также перебазируются снимки undo/redo. Это изменение
представления не добавляет отдельную запись истории. Нет автоматического
создания каталогов, перезаписи будущих версий или восстановления повреждённых
файлов. Неуспешные open/save оставляют Session неизменной.

Run использует immutable runtime-копию и revision проекта. Результаты содержат
project_revision для аудита снимка; Session определяет устаревание по отдельной
computation revision. Move/Rename и их Undo не инвалидируют данные; изменения
параметров/связей/состава графа/designated output требуют повторного Run.
WorkerConfig, JobManager и последние результаты — transient state. Для несохранённого
проекта относительные источники выбранной ветки требуют сначала SaveAs.
Абсолютные локальные пути допустимы без сохранения, что используется Inspect.

## §5. Реализация

crates/datacraft-engine/src/commands.rs — типы и registry;
src/session.rs — application facade, dispatch и history;
src/lib.rs — существующий Spark Worker. CLI open/validate/run/inspect использует
Session. Внешние клиенты не получают mutable Project.

Пример использования из Rust-функции с Result:

```rust
let mut session = datacraft_engine::Session::default();
session.dispatch(datacraft_engine::Command::Open {
    path: "examples/pipelines/sample.dcraft".into(), discard_changes: false,
})?;
session.dispatch(datacraft_engine::Command::Rename { name: "Experiment".into() })?;
session.dispatch(datacraft_engine::Command::Undo)?;
```

Discovery CLI: `cargo run --locked -p datacraft-cli -- commands --json`.

## §6. Замечания

CommandResult возвращает структурированный payload, revision, dirty и events.
События относятся к завершённой команде и не накапливаются в бесконечной очереди.
Availability описывает предварительную доступность, не заменяет validation
в dispatcher и проверку реального filesystem/runtime.

execution.run возвращает JobStarted без ожидания запроса к JVM. Контракт
переходов состояний, progress stages, timeout, cancellation и API
poll_execution/wait_for_execution описан в EXECUTION.md. Команды cancel/status
исполняемы; проектные события и execution events — отдельные каналы API.
CommandResult.changed относится к изменению project revision, а не job state.

## §7. Приёмка

Тесты команд, JSON tags, metadata consistency, rollback ошибочных/пакетных правок,
undo/redo, no-op, branching history, ограничение стека, dirty checkpoints,
удаление/восстановление рёбер и output, open/save failures и SaveAs rebasing.
Прежние CLI/Spark fixtures должны исполняться через Session.
Фактический результат команд записывается в VALIDATION.md.
Текущие результаты последующих итераций находятся в VALIDATION.md.

## §8′. Устойчивость

Проверка данных через Result, без panic на ошибках пользователя. Снимки могут
быть дорогими для большого графа; лимит истории настраиваемый. Работа с файлами
использует прежний atomic save Project. Ошибка запуска не откатывает проект,
но и не добавляет историю. Save не отменяется через undo; открытие другого
проекта не подменяет историю старого проекта. UI selection/zoom/focus здесь нет.
