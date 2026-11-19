# SubZir player guide

SubZir reads a game's English subtitles and displays their translation in your OBS scene. For a PC game you need OBS Studio. For a PS5 you also need a capture card that shows the console picture in OBS.

## 1. Get the edition for your PC

- **RTX 5070 Ti** — download every part of `SubZir-7B-RTX-5070-Ti.zip` from the same release.
- **RTX 3080** — download every part of `SubZir-7B-RTX-3080.zip`.
- **CPU translation** — download `PS5-Subtitles-Windows-x64.zip`.

Put all parts of your edition in one folder. In 7-Zip open the file ending in **.zip.001**, click **Extract** and choose a folder in Documents. If you already have a complete **.zip**, use **Extract All**. Open **SubZir.exe** in the extracted **SubZir** folder. Both GPU editions include the 7B model. On first launch with Ukrainian output, look for **Hy-MT2 7B Q4_K_M (GPU)** at the bottom of the window.

![SubZir showing the three setup buttons, the selected 7B model and the subtitle appearance control](images/launcher-7b-en.png)

## 2. Bring the game into OBS

1. Open the game and turn on **English subtitles** in its settings.
2. Select your game scene in OBS. For a **PC game**, choose **Sources → + → Game Capture**, select **Capture specific window** and pick the game. For a **PS5**, add **Video Capture Device** and select your capture card. Check that the game picture is visible.
3. Open the settings next to **Start Virtual Camera**. Set the output to **Source**, pick your game or capture card and start the virtual camera. SubZir now receives the original game subtitles.

## 3. Test translation and select the text

1. Click **Language** in SubZir and choose the interface and subtitle languages separately.
2. Click **1. Connect OBS**. In OBS open **Tools → Scripts → +**, paste the copied path and open the Lua file. Click **Add subtitles to the current scene** in the script settings. When upgrading, use the script from your new SubZir folder.
3. Click **2. Test translation**. The translation of “Open the gate.” appears in the OBS scene for about four seconds. You can test this before connecting a console.
4. Wait for a character to speak. Click **3. Select subtitle area**, draw a box around the English subtitle with room for two lines, then click **Save area**. In the screenshot below, select the upper line that says “Sindri’s been helping me.”
5. Click **Start translation**. Use **Subtitle appearance** to change the font, outline and background. To play with the translation full screen, right-click the scene in OBS, choose **Open Scene Projector** and select your monitor.

![Atreus speaking in God of War Ragnarök, with the Ukrainian translation under the English subtitle in OBS](images/god-of-war-obs.jpg)

## If the translation is missing

- **The test line never appeared.** Open the correct OBS scene. Make sure the subtitle source is visible and sits above the game source in the Sources list.
- **The test worked but game dialogue is missing.** Check the virtual camera source. Draw the box again while a subtitle is visible. Repeat this after a game resolution change.
- **7B fails to start.** Check that all parts match your GPU edition, update your NVIDIA driver and restart SubZir. Select **Automatic** to use CPU translation while troubleshooting. Details are in `runtime/diagnostics.txt`.
