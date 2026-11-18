# SubZir для OBS

SubZir reads English subtitles from your PS5 capture card feed and displays the Ukrainian translation in your OBS scene. Select the subtitle area once and the app follows each new line automatically. It runs offline on your Windows PC using the bundled QuickMT model.

[Download for Windows 10/11 x64](https://github.com/Daelon02/subzir-obs/releases/latest/download/PS5-Subtitles-Windows-x64.zip)

SubZir читає англійські субтитри з вибраної області гри й показує український переклад у сцені OBS. Після першого налаштування переклад працює автоматично й локально на CPU.

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
