# SubZir для OBS

[English setup guide](#first-launch) · [Інструкція українською](#перший-запуск)

SubZir reads subtitles from a selected area of your capture card feed and displays each translated line in your OBS scene. Translation runs offline on your Windows PC. The v0.1.0 download uses English text recognition in Windows and includes a QuickMT model for English-to-Ukrainian translation.

[Download for Windows 10/11 x64](https://github.com/Daelon02/subzir-obs/releases/latest/download/PS5-Subtitles-Windows-x64.zip)

## First launch

You need OBS Studio and Windows 10/11 x64 with English text recognition installed. The ZIP includes the translation model. The launcher uses Ukrainian button labels, shown below so you can find them on screen.

1. Extract the entire ZIP to a folder, then open **Субтитри PS5.exe**.
2. In OBS, select the scene you use for the game and add your capture card as a **Video Capture Device**. Turn on English subtitles in the game.
3. In SubZir, click **1. Під’єднати OBS** (Connect OBS) to copy the script path. In OBS, open **Tools → Scripts**, click **+**, and paste the path into the file picker. In the script settings, click **Додати субтитри до поточної сцени** (Add subtitles to the current scene).
4. Click **2. Перевірити переклад** (Test translation) in SubZir. You should see **Відчиніть ворота.** (“Open the gate.”) in your OBS scene. You can run this test before connecting the PS5.
5. In OBS, click the gear next to **Start Virtual Camera**, set **Output Type** to **Source**, and choose your capture card. Start the virtual camera.
6. When English dialogue appears, click **3. Виділити субтитри** (Select subtitle area). Drag a box around the English text, leaving room for two lines. Click **Зберегти область** (Save area), then **Почати переклад** (Start translation). Next time, start the virtual camera and click **Почати переклад** to reuse the saved area.

## For developers

The Rust engine in `src/` reads English subtitle text with Windows OCR. When running the engine directly, `SUB_WORKER` selects the Python translation worker. A compatible worker can return text in a different target language. The Windows launcher selects the bundled QuickMT English-to-Ukrainian worker, which uses CTranslate2 on the CPU. The C# WinForms launcher controls the session, and the Lua script displays the translation in OBS.

To build the engine on Windows, install the Rust MSVC toolchain and Visual Studio C++ Build Tools. From the repository root, run:

```powershell
cargo build --release --locked
```

To try changes in the full app, extract the Windows release into a test folder. Pass the folder containing **Субтитри PS5.exe** to the build script:

```powershell
.\scripts\build.ps1 -BundleDirectory '.\PS5-Subtitles'
```

The script rebuilds the engine and launcher, copies the updated scripts into the test folder, and uses the Python runtime, FFmpeg, and model from the release. To check the result, open the rebuilt launcher and run the translation test in step 4.

## Українською

SubZir зчитує субтитри з вибраної області відео з карти захоплення й показує переклад кожної репліки у сцені OBS. Переклад працює локально на ПК. Збірка v0.1.0 використовує розпізнавання англійського тексту у Windows і містить модель QuickMT для перекладу з англійської на українську.

**[Завантажити для Windows x64](https://github.com/Daelon02/subzir-obs/releases/latest/download/PS5-Subtitles-Windows-x64.zip)**

## Перший запуск

1. Розпакуйте весь архів і відкрийте **Субтитри PS5.exe**.
2. В OBS додайте карту захоплення як джерело. У грі увімкніть англійські субтитри.
3. У програмі натисніть **1. Під’єднати OBS**. В OBS відкрийте **Інструменти → Скрипти**, додайте файл **PS5-Subtitles.lua** і натисніть **Додати субтитри до поточної сцени**.
4. Натисніть **2. Перевірити переклад**. У сцені OBS має з’явитися «Відчиніть ворота.». Ця перевірка працює і без консолі.
5. У налаштуваннях віртуальної камери OBS виберіть вихід **Джерело** та свою карту захоплення. Запустіть віртуальну камеру.
6. Коли на екрані є діалог, натисніть **3. Виділити субтитри**, обведіть місце для одного або двох рядків і збережіть область. Натисніть **Почати переклад**.

Потрібні Windows 10/11 x64, компонент Windows для розпізнавання англійського тексту та OBS Studio. Перекладач і модель QuickMT EN→UK вже є в архіві. [Повна інструкція та вирішення проблем](docs/PLAYER-UA.txt)

## Для розробників

Рушій у `src/` використовує Windows OCR. Переклад виконує локальний Python-процес із CTranslate2 та QuickMT, графічний інтерфейс написаний на C# WinForms, а текст у сцені оновлює Lua-скрипт OBS.

Щоб зібрати рушій на Windows із Rust MSVC та Visual Studio C++ Build Tools, виконайте `cargo build --release --locked`. Для перевірки змін у повному застосунку розпакуйте реліз у тестову папку та виконайте в PowerShell з кореня репозиторію:

```powershell
.\scripts\build.ps1 -BundleDirectory '.\PS5-Subtitles'
```

Скрипт збирає рушій та інтерфейс і оновлює їх разом зі скриптами в указаній копії. Вбудовані Python, FFmpeg та модель бере з розпакованого комплекту. [Сторонні компоненти](docs/COMPONENTS.txt)

## License / Ліцензія

Daelon02 licenses the original SubZir code and documentation under the MIT License. You may copy, modify, and redistribute them, including commercially. Include the Daelon02 copyright notice and the license text in all copies or substantial portions. Bundled third-party components retain their own licenses. [MIT License](LICENSE) · [Third-party notices](docs/COMPONENTS.txt)

Оригінальний код і документацію SubZir можна копіювати, змінювати й поширювати, зокрема комерційно. У кожній копії або суттєвій частині зберігайте повідомлення про авторство Daelon02 та текст ліцензії MIT. Для сторонніх компонентів діють їхні власні ліцензії.
