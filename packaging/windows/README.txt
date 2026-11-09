Bolo {version}: command-line preview for Windows x64
=====================================================

Bolo is open-source, local-first voice dictation for English and Hinglish.
This zip holds the `bolo` command-line tool: try live dictation from a
terminal and benchmark speech-to-text engines on your own voice. The desktop
app and the Android keyboard come in later releases.

Source, docs and issues: https://github.com/Nirav1810/bolo

This is a preview build. It is not code-signed yet, so Windows SmartScreen
may warn the first time you run it.


Quick start
-----------

Unzip the folder anywhere and open a terminal (PowerShell or cmd) in it.
Keep bolo.exe next to the .dll files; it needs them.

    .\bolo.exe sysinfo                      # RAM and the recommended model
    .\bolo.exe download nemotron-stream     # fetch and verify an on-device model
    .\bolo.exe dictate                      # Enter = start/stop; text goes to the clipboard

Models are downloaded on demand from their upstream sources, checked against
a SHA-256, and stored in %LOCALAPPDATA%\Bolo\data\models (change it with
--models-dir). Nothing leaves your computer unless you pick a cloud engine.

`.\bolo.exe help` lists every command; `.\bolo.exe help <command>` explains one.


Cloud engines (optional, bring your own key)
--------------------------------------------

Put keys in a file named .env in the folder you run bolo from:

    DEEPGRAM_API_KEY=...     # -e deepgram or -e deepgram-stream
    GROQ_API_KEY=...         # -e groq
    SARVAM_API_KEY=...       # -e sarvam
    ANTHROPIC_API_KEY=...    # dictate --polish

then, for example:  .\bolo.exe dictate -e deepgram-stream


Files
-----

bolo.exe                        the CLI
sherpa-onnx-*.dll, onnxruntime*.dll
                                on-device speech recognition (sherpa-onnx,
                                ONNX Runtime)
LICENSE-MIT.txt, LICENSE-APACHE.txt
                                Bolo is dual-licensed MIT OR Apache-2.0
THIRD-PARTY-NOTICES.txt         licences of the bundled libraries
