# SubZir for OBS

Translate English subtitles from a game running on your Windows PC or from a console connected through a capture card. Select the game's subtitle area once. SubZir shows each translated line in OBS and clears it when the text disappears.

**[Download SubZir for Windows x64](https://github.com/Daelon02/subzir-obs/releases/latest/download/PS5-Subtitles-Windows-x64.zip)** · [Українською](#перший-запуск) · [PS5 benchmark](docs/benchmarks/PS5-EN.md)

![Atreus tells Kratos about Sindri in God of War Ragnarök, with the English line above its Ukrainian translation in OBS](docs/images/god-of-war-obs.jpg)

## First launch

1. Extract the entire ZIP to Documents and open **SubZir.exe**. Click **Language** to choose the **interface language** and **subtitle language** independently. Both lists have 100 languages. English subtitle output shows the original recognized line.
2. Enable **English subtitles** in the game. Choose how to bring its picture into your OBS scene.
   - **Game running on this PC.** Launch the game. In OBS, go to **Sources → + → Game Capture**, choose **Capture specific window**, then select the game window. This works without a console or capture card.
   - **Console.** Add the capture card as a **Video Capture Device**.
3. Click **1. Connect OBS** in SubZir. In OBS, open **Tools → Scripts → +**, paste the copied path and open the Lua file. Click the button in its settings to add subtitles to the scene.
4. Click **2. Test translation**. A translation of “Open the gate.” appears in OBS for four seconds. You can try this before connecting the console.
5. In OBS, open the settings beside **Start Virtual Camera**. Set the output to **Source**, select the Game Capture or capture-card source you added in step 2, then start the virtual camera. SubZir now sees the original game picture.
6. When dialogue appears, click **3. Select subtitle area**. Draw a box around the English text, leave room for two lines and save. Click **Start translation**. To switch the output while playing, use **Language**. The next new line uses your selection.

**Where will I see the translation while playing on PC?** The game window shows its original subtitles. The OBS scene shows the translated overlay, even with streaming and recording turned off. To see that scene full screen, right-click it in the **Scenes** list, choose **Open Scene Projector** and select a monitor. If you have two monitors, you can keep the game focused on one and the translated view on the other.

Requires Windows 10/11 x64, Windows English OCR and OBS Studio. Both models run locally on the CPU. English and Ukrainian have edited interface text. The other localizations are machine-assisted and their quality varies. Some scripts require supplemental Windows fonts. [Player guide and troubleshooting](docs/PLAYER-EN.txt) · [Supported languages](launcher/languages.json)

![SubZir launcher in English](docs/images/launcher-en.png)

## Перший запуск

SubZir зчитує англійські субтитри з гри на ПК або з консолі, під’єднаної через карту захоплення, й показує переклад у сцені OBS. Один застосунок має 100 мов меню й 100 варіантів для субтитрів. Наприклад, можна залишити меню українською й показувати переклад польською. Обидва налаштування зберігаються.

1. Розпакуйте весь ZIP у «Документи» й відкрийте **SubZir.exe**. Натисніть **Мова / Language** та виберіть окремо мову інтерфейсу й мову субтитрів.
2. У грі ввімкніть **англійські субтитри**. Додайте зображення гри до сцени OBS.
   - **Гра на цьому ПК.** Запустіть гру. В OBS відкрийте **Джерела → + → Захоплення гри**, виберіть режим захоплення окремого вікна, а потім вікно гри. Для цього достатньо самого ПК.
   - **Консоль.** Додайте карту захоплення як **Пристрій захоплення відео**.
3. У SubZir натисніть **1. Під’єднати OBS**. В OBS відкрийте **Інструменти → Скрипти → +**, вставте скопійований шлях і відкрийте файл. Натисніть кнопку додавання субтитрів у налаштуваннях скрипту.
4. Натисніть **2. Перевірити переклад**. На чотири секунди у сцені з’явиться переклад «Open the gate.» вибраною мовою. Для цього тесту консоль можна під’єднати пізніше.
5. У налаштуваннях віртуальної камери OBS виберіть вихід **Джерело**, потім джерело з кроку 2 — захоплення гри або карту захоплення. Запустіть віртуальну камеру. Тепер SubZir бачить вихідне зображення гри.
6. Коли з’явиться репліка, натисніть **3. Виділити субтитри**, обведіть область для двох рядків і збережіть її. Натисніть **Почати переклад**. Після перемикання мови наступна нова репліка з’явиться вибраною мовою.

**Де я побачу переклад під час гри на ПК?** У вікні гри видно оригінальні субтитри. Переклад з’являється у сцені OBS навіть без стріму чи запису. Щоб розгорнути сцену на весь екран, клацніть її правою кнопкою в списку **Сцени**, натисніть **Відкрити проєктор сцени** й виберіть монітор. Якщо маєте два монітори, можна грати на одному й бачити сцену з перекладом на другому.

Потрібні Windows 10/11 x64, компонент розпізнавання англійського тексту й OBS Studio. Для української комплект містить QuickMT, для інших перекладів — M2M100. Англійський режим показує розпізнаний оригінал. [Інструкція для гравця](docs/PLAYER-UA.txt) · [Заміри на підключеній PS5](docs/benchmarks/PS5-UK.md)

![Український інтерфейс SubZir](docs/images/launcher-uk.png)

![Українське меню з польськими субтитрами у вікні вибору мов](docs/images/languages.png)

## For developers / Для розробників

Rust runs the capture, OCR, stability filter and overlay timing. The WinForms launcher writes both language choices to `runtime/languages.json`. The Python worker checks the target on each new input. It reuses the loaded CTranslate2 model across lines. The OBS Lua script updates a text source. Set `SUB_TARGET` to a code from `launcher/languages.json` to override the subtitle language when running the worker directly.

On Windows, install Rust MSVC and Visual Studio C++ Build Tools. Extract the **v0.2.0** Windows bundle to a test directory to supply embedded Python, FFmpeg and both model directories. From the repository root run the following commands.

```powershell
cargo build --release --locked
.\scripts\build.ps1 -BundleDirectory '.\PS5-Subtitles'
```

Для перевірки змін розпакуйте комплект v0.2.0, виконайте наведені команди й запустіть **SubZir.exe** у тестовій папці. Натисніть **Перевірити переклад**. Локалізації містяться в `launcher/locales`. Перелік кодів відповідає мовним токенам моделі. [Вимірювання та дані](docs/benchmarks/PS5-UK.md) · [Model and runtime notices](docs/COMPONENTS.txt)

## License / Ліцензія

Daelon02 licenses original SubZir code and documentation under MIT. You may copy, modify and redistribute them, including commercially. Keep the copyright notice and license text in copies or substantial portions. Сторонні компоненти поширюються за власними ліцензіями. [MIT](LICENSE) · [Third-party notices](docs/COMPONENTS.txt)
