#!/bin/sh
# Ставит Gargantua для текущего пользователя: бинарь в ~/.local/bin, ярлык и иконку — в меню приложений
set -e
cd "$(dirname "$0")"
mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/256x256/apps
install -m755 blackhole ~/.local/bin/blackhole
install -m644 blackhole.png ~/.local/share/icons/hicolor/256x256/apps/blackhole.png
install -m644 blackhole.desktop ~/.local/share/applications/blackhole.desktop
echo "Gargantua установлен: ~/.local/bin/blackhole (и в меню приложений)."
for b in mpv yt-dlp python3; do
  command -v "$b" >/dev/null 2>&1 || echo "Не хватает: $b — поставь его пакетным менеджером."
done
