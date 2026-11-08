# Файловые диалоги и имя проекта

## §1. Задача

Выбирать `.dcraft`, CSV и Parquet системным диалогом; сохранять проект под новым
именем файла и изменять его отображаемое имя. Постоянные изменения идут через
Session; ручной ввод путей остаётся доступен без работающего native backend.

## §2. Алгоритм

UI ставит один запрос выбора в очередь. В native App::update rfd AsyncFileDialog
создаётся на GUI thread с parent window. Future опрашивается без block_on;
Wake запрашивает repaint. Выполнение Spark и polling продолжаются.
Результат проверяется по revision проекта и UUID/параметру инспектора.
Open вызывает существующую защиту dirty state, Save as — Session::SaveAs,
источник меняет только поле буфера. Rename вызывает Session::Rename.

## §3. Применения

Open… открывает системный выбор проекта; Open path использует текстовую строку.
Save сохраняет текущий файл, для нового проекта открывает Save as….
Save as… выбирает новый файл, Save path сохраняет по введённому пути.
Browse… у источника выбирает CSV/Parquet; затем нужен Apply. В Rename вводится
новое имя проекта; Undo/Redo восстанавливают имя через обычную историю.

## §4. Контракт

Одновременно открыт не более чем один native dialog. Графовые правки и смена
проекта на это время отключены; jobs продолжаются. Отмена не меняет проект,
буферы, историю и checkpoint, в том числе при Save and close нового проекта.
Новый путь источника сохраняется абсолютным и требует валидного UTF-8;
ручные относительные пути разрешаются от каталога `.dcraft` как прежде.
Неподтверждённый путь не применяется автоматически. New/Open/SaveAs не обходят
защиту будущих версий проекта. Запоздавший результат не применяется к другому
проекту/ноде. Пока native dialog открыт, закрытие родительского окна отклоняется.
Rename не изменяет имя файла; пустое имя отвергает engine, без изменения истории.

## §5. Реализация

rfd 0.18.0, MIT, MSRV 1.92; default features xdg-portal/wayland.
API/features проверены по исходникам tag 0.18.0:
[Cargo.toml](https://github.com/PolyMeilex/rfd/blob/0.18.0/Cargo.toml),
[AsyncFileDialog](https://github.com/PolyMeilex/rfd/blob/0.18.0/src/file_dialog.rs).
`file_dialog.rs` содержит временные запросы и future, `editor.rs` — Browse,
`lib.rs` — маршрутизацию результатов, Rename и команды Session.
Новый UI dependency не допускается в headless слоях: xtask layers запрещает rfd.

## §6. Замечания

Windows использует системный picker; macOS требует работающий NSApplication
(создаётся eframe). Linux: XDG Desktop Portal с GTK/GNOME/KDE backend либо
Zenity; нужны системные библиотеки D-Bus. Подробности backend —
[документация rfd](https://docs.rs/rfd/0.18.0/rfd/).
Нет обязательных сетевых сервисов. Полный путь выбирается локально.

## §7. Приёмка

Проверки: cancelled/ready/pending future, отказ повторного picker, сохранение
буфера и Project при отмене, stale revision/UUID, источник только в буфере до
Apply, Save as round-trip и rebasing, Save and close при отмене, Rename/Undo/Redo,
пустое и кириллическое имя. Adapter-тесты подают ответы selector явно и не
выдаются за native OS-dialog tests. Native запуск и ручная приёмка отмечаются
отдельно в VALIDATION.md.

## §8′. Устойчивость

Отсутствие результата rfd не различает Cancel и недоступный backend: UI сообщает
об отсутствии выбора и предлагает ручной путь. Диалог не пишет файл сам: запись
выполняет Session после выбора. Save as отменён до Apply — буфер остаётся прежним.
Внешний force-close ОС всё ещё может потерять несохранённые правки.
