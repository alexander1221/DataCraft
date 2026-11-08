# Первый desktop-клиент

## §1. Задача

Нативное окно создаёт/открывает `.dcraft`, показывает и редактирует ноды, параметры
и связи, сохраняет граф и выполняет preview. Контракт редактирования и защиты
буфера: [GRAPH_EDITOR.md](GRAPH_EDITOR.md).

## §2. Алгоритм

Desktop владеет Session. Open/Save/SaveAs, завершение перемещения и Undo/Redo
вызывают её команды. Во время drag позиция временная: одна завершённая операция
попадает в историю один раз. Canvas строится непосредственно из immutable графа,
без второй постоянной копии. Pan/zoom, selection и drag — UI state.
Run создаёт background job; окно опрашивает события и результат, не ждёт JVM.

## §3. Применения

```sh
cargo run -p datacraft-desktop -- examples/pipelines/sample.dcraft
cargo run -p datacraft-desktop -- --help
cargo run -p datacraft-desktop -- examples/pipelines/sample.dcraft --smoke-frames 3
```

Open… и Save as… открывают системные диалоги; Open path/Save path используют
введённый путь. Save сохраняет текущий файл, а для нового проекта предлагает
Save as. Browse… выбирает источник, Apply записывает его в граф. Rename project…
меняет отображаемое имя через Session с Undo/Redo, не переименовывая файл.
Детальный контракт: [FILE_DIALOGS.md](FILE_DIALOGS.md).
New project начинает пустой граф. Кнопка типа в Nodes добавляет ноду, Delete node
удаляет выбранную. Параметры изменяются через Apply/Revert. Для соединения нажмите
выходной порт, затем входной; Disconnect находится в Connections инспектора.
Escape/правая кнопка отменяют временный провод. Set project output назначает
выбранный Preview, Linear Regression report, Regression Metrics или Classification
Metrics выходом проекта.
Средняя кнопка перемещает canvas, колесо меняет масштаб, Fit показывает граф.
Arrange под Nodes разносит DAG по слоям и веткам через один Batch Move с Undo.
Open сохраняет геометрию пользователя. Примеры fit/transform уже разнесены;
Model-связи золотые, Table-связи синие. Перемещение и Rename не делают результат
устаревшим. Контракт и ограничения: [EDITOR_READABILITY.md](EDITOR_READABILITY.md).
Нажатие ноды выбирает её, drag перемещает. Run selected запускает выбранный выход;
Run project использует designated output. Cancel останавливает текущий JVM.

## §4. Контракт

eframe/egui 0.33.3, backend Glow; версия фиксирована ради проверенного API,
это не утверждение о последней версии. UI crate объявляет layer=ui и зависит
от core/engine, не от runtime или DataFusion. Формат проекта остаётся v1.
Пути Java и classpath задаются флагами или DATACRAFT_* как в CLI. Относительные
пути флагов разрешаются от текущего каталога; источники — от проекта.
Открытие другого проекта с правками и выход требуют явного discard либо Save.
При активном job выход запрашивает Cancel и дожидается terminal state через polling.

## §5. Реализация

`apps/datacraft-desktop`: startup/CLI в main.rs, окно и Session adapter в lib.rs,
геометрия canvas в canvas.rs, типизированный буфер параметров в editor.rs,
асинхронные native requests/future в file_dialog.rs. Тесты UI adapter — tests.rs
и tests/file_dialogs.rs; ответы picker в них подаются явно.
Используются публичные API eframe tag 0.33.3
([App](https://github.com/emilk/egui/blob/0.33.3/crates/eframe/src/epi.rs)).
MIT OR Apache-2.0 — лицензия выбранного eframe/egui. Сторонние code/assets
не копируются. Canvas и портовые жесты используют egui Painter; egui-snarl
в текущую реализацию не входит. Единственная постоянная модель находится в Session.

## §6. Замечания

Открытие/сохранение JSON синхронные; Spark работает в JobManager. Результаты
ограничены движком, таблица прокручивается; схема читается из ответа worker.
Нет автосохранения и восстановления UI state. Ручной путь остаётся альтернативой
native picker; закрытие родителя с открытым picker отклоняется до его завершения.

## §7. Приёмка

Проверки сборки, тестов и настоящего окна фиксируются в VALIDATION.md.
Сценарий ручной приёмки: открыть sample, выбрать Filter, переместить ноду,
Undo/Redo, Save as во временный каталог, переоткрыть, Run project:
quantity/amount = (4,11), (5,13), (6,15). Убедиться, что окно остаётся отзывчивым,
Cancel завершается, следующий запуск работает, dirty-close защищает правки.
Headless egui frames проверяют генерацию интерфейса, но не заменяют native smoke.
Отдельная приёмка кнопки Run через egui input и настоящий Spark (нужен worker):

```sh
cargo test -p datacraft-desktop --features spark-integration-tests run_button_executes_real_spark_and_preview_is_rendered
```

Ожидания: исходные три CSV-строки и отрисованный Preview; после редактирования
Filter на `quantity >= 5` через инспектор и Apply второй Run возвращает
(5,13)/(6,15)/(7,17). New после завершения очищает старый результат/job.
Тест использует DATACRAFT_JAVA и DATACRAFT_WORKER_CLASSPATH, если они заданы.
Это headless UI-тест с настоящим backend, не имитация успешного ответа.

## §8′. Устойчивость

Неверный путь/JSON/выход даёт видимую ошибку без замены предыдущего проекта.
Отмена открытия не сбрасывает выбор и viewport. Закрытие не сохраняет молча.
Позиции конечны, масштаб ограничен 0.2–2.0, история остаётся ограниченной Session.
Проекты не являются песочницей: запуск читает указанные локальные файлы.
