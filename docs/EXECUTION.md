# Фоновые задания

## §1. Задача и инварианты

Execution.run не выполняет JVM/query на вызывающем потоке. Один Session допускает
один активный job. Проект и его revision фиксируются на момент запуска; правки
и undo во время выполнения не меняют snapshot. Execution state не сериализуется
в .dcraft и не входит в undo history.

## §2. Алгоритм

Session делает структурный preflight выбранной ветки и разрешает пути. JobManager
публикует Queued и запускает два потока: runner и supervisor. Runner создаёт
Worker, регистрирует cancellation handle, читает ответ и закрывает JVM.
Supervisor ждёт Condvar, отменяет зарегистрированный процесс при cancel/timeout.
Ни один mutex не удерживается во время запроса Worker::run.
Только после cleanup публикуется Completed/Failed/Cancelled/TimedOut.

## §3. Применения

GUI/MCP может dispatch Run, получить Job ID, продолжить редактирование и polling.
CLI использует тот же Run и блокирующий wait вне GUI. Отмена работает при старте
JVM и во время запроса; следующий job разрешён после terminal status.
Каждый job использует собственный JVM, без пула и повторного использования.

## §4. Контракты

JobInfo: UUID id, project_revision, status, optional error и termination_error.
Статусы: queued → running → completed/failed; запрос stop даёт cancelling,
после cleanup — cancelled либо timed_out. Отмена до публикации terminal status
побеждает результат; запрос после terminal ничего не меняет.
Первый stop reason побеждает при гонке cancel/timeout. Повторный cancel идемпотентен.

JobOptions.timeout: default 300 секунд, None отключает deadline, zero — ошибка.
Timeout — время от принятия задания, включая запуск JVM. Остановка принудительная:
локальный процесс JVM завершается целиком; это не Spark job-group cancellation.
Ошибка kill сохраняется как termination_error; задание остаётся активным до
реального завершения runner. Нельзя выдавать неподтверждённую остановку за terminal.

События: queued, started, stage, cancellation_requested, completed, failed,
cancelled, timed_out. Stage — starting_worker/executing/stopping_worker,
не процент и не счётчик обработанных строк. Очередь ограничена 64 событиями
текущего job; при переполнении удаляются старейшие, статус остаётся доступен.
Менеджер хранит только текущий/последний job; новый заменяет terminal job.
Потребитель сохраняет нужные результаты самостоятельно через Arc.

Command execution.run возвращает JobStarted { job_id, project_revision };
execution.cancel принимает job_id; execution.status возвращает JobInfo.
Ошибки preflight возвращаются синхронно без Job ID. Ошибки JVM/query — в JobInfo.
Session.poll_execution обновляет last_execution и возвращает события.
job_result даёт Arc к bounded результату, без копирования таблиц под mutex.
wait_for_execution — блокирующий convenience API для CLI/тестов, не GUI.

Project.open запрещён, пока job активен. Правки, undo/redo, save/save_as разрешены.
Runtime config изменяет только будущие jobs. Старый результат после правки
доступен, но result_is_current=false после изменения вычислений. Move/Rename
не инвалидируют результат: Session отдельно отслеживает computation revision,
захваченную при Run. Computational Undo/Redo консервативно требуют нового Run.
Project revision остаётся в record для аудита. Ревизии не обнаруживают внешнее изменение
dataset. Конфигурация и история проекта не меняются из-за отмены или ошибки query.

## §5. Реализация

jobs.rs — scheduling, observable state, events, stop/deadline; Worker cancellation
handle в lib.rs — короткая операция над Child; session.rs — command facade.
CLI --timeout-seconds задаёт deadline, затем ждёт terminal status и печатает
таблицы или возвращает nonzero для failed/cancelled/timed_out.

## §6. Замечания

Runner и supervisor используют стандартную библиотеку Rust; async runtime
не добавляется. Shared status/result защищены короткими mutex; poison восстанавливается,
panic runner превращается в failed. Результат bounded прежними лимитами Worker.
Drop менеджера запрашивает best-effort cancel; host должен дождаться shutdown
перед завершением процесса, если требуется подтверждённая очистка JVM.

## §7. Приёмка

Детерминированные unit tests с управляемым runner проверяют гонки, cancel до
регистрации worker, timeout, busy, terminal events и revision, без выдачи stub
за Spark. Отдельная проверка Child kill использует реальный дочерний процесс.
Реальные CLI/Spark fixtures подтверждают production runner. Results: VALIDATION.md.

Пример для настоящего JVM:

```sh
cargo run --locked -p datacraft-engine --example job_lifecycle -- examples/pipelines/sample.dcraft
```

Он ждёт регистрации worker, отправляет execution.cancel, ждёт подтверждённой
очистки, запускает следующий job и редактирует Filter, пока работает
snapshot. Затем проверяет stale revision. Это не имитация Spark.

## §8′. Устойчивость

Нет гарантий мгновенного прерывания системного spawn/чтения classpath или kill
при отказе ОС. Poll/cancel не выполняют файловый/сетевой I/O. Preflight графа
и разрешение пути остаются синхронными. Нет распределённых executors и subprocess
деревьев; реализация останавливает именно свой локальный JVM PID.
Полноценные метрики Spark, Ctrl+C-handler, пул JVM и persist job history не входят
в этот этап. Неизвестный/заменённый Job ID возвращает structured error.
