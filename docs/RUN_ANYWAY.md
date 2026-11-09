# Open HOI4 Mod Setup past the Windows and macOS warnings

HOI4 Mod Setup is free and open source. Its builds are not signed with a paid certificate, so the first time you open a downloaded installer, Windows and macOS stop and ask you to confirm. This is expected. It does not mean the file is damaged.

Download only from the [official releases page](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest) or the links in the [README](../README.md). Then follow the steps for your system. The window titles and buttons below are the ones Windows 11 and macOS 15 and 26 show. Older versions word them slightly differently.

## Windows: SmartScreen

Windows shows **Windows protected your PC** because the installer comes from a publisher it does not know yet. Published builds are signed with a community certificate named ChaosX, which Windows does not trust by default ([signing details](../RELEASING.md#signing-configuration)).

1. Double-click `HOI4-Mod-Setup-windows-x64-setup.exe` in your Downloads folder.
2. In the blue **Windows protected your PC** window, click **More info**. Do not click **Don't run**.
3. Check that the file name is the installer you downloaded. The publisher reads `ChaosX`.
4. Click **Run anyway**.
5. Answer the user account control question with **Yes** and finish the installer.

If your browser stops the download first, open its downloads list, choose **Keep** (or **Keep anyway**) for the installer, then continue with step 1. Edge and Chrome say the file "isn't commonly downloaded" for the same reason.

Windows remembers your choice for that file. You only do this again when you download a new installer.

## macOS: Open Anyway

macOS blocks the first launch because the app carries an ad-hoc signature and is not notarized by Apple ([signing details](../RELEASING.md#signing-configuration)).

1. Open the downloaded `.dmg`. Its window shows only **HOI4 Mod Setup**, with no Applications shortcut, so open a second Finder window, choose **Go**, then **Applications**, and drag **HOI4 Mod Setup** into it. Use the **Apple silicon** disk image on a Mac with an M-series chip and the **Intel** one on an older Mac.
2. Eject the disk image, then open **HOI4 Mod Setup** from **Applications**. macOS shows a dialog that says it cannot verify the app. Click **Done**. Do not click **Move to Trash**.
3. Open the Apple menu, then **System Settings**, then **Privacy & Security**.
4. Scroll down to **Security**. A line says the app was blocked. Click **Open Anyway**.
5. Enter your Mac password or use Touch ID. Click **Open Anyway** once more in the dialog that follows.

When Setup first prepares a project, macOS asks whether **HOI4 Mod Setup** may access files in your **Documents** folder. Click **Allow**. Hearts of Iron IV keeps its mod folder there, so Setup cannot create or update a mod without it. If you clicked **Don't Allow**, turn it on again in **System Settings**, then **Privacy & Security**, then **Files & Folders**.

The **Open Anyway** button appears for about an hour after the blocked attempt. If it is gone, open the app again from **Applications** and return to **Privacy & Security**.

On macOS 14 and earlier you can also hold **Control**, click the app in **Applications**, choose **Open**, and click **Open** again. macOS 15 and later no longer offers this shortcut, so use the steps above.

## If something still does not open

- A message that the file is damaged usually means an incomplete download. Delete it and download it again.
- A work or school computer may forbid unsigned apps. Ask the person who manages it, or use the [web edition](https://klimpaskov.github.io/HOI4-Mod-Setup/app/), which installs nothing.
- Linux shows no warning. See the [Linux guide](LINUX.md).
- For anything else, open an [issue](https://github.com/klimPaskov/HOI4-Mod-Setup/issues) and include your Windows or macOS version.
