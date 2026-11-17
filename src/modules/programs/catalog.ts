/** Curated programs for Programs → Install. Every id was checked in the winget catalog.
 *  Icons live in public/apps/<key>.png. Not included on purpose: Yandex (owner's choice).
 *  Ids without a dot are Microsoft Store products (winget --source msstore).
 *  Apps in neither source (Spotify, AMD) open their official download page instead. */
export type CatalogCategory = "browsers" | "chat" | "games" | "drivers" | "components" | "media" | "graphics" | "docs" | "utils" | "hardware" | "dev";

export interface CatalogApp {
  key: string;
  name: string;
  cat: CatalogCategory;
  /** winget package ids; several = installed together (e.g. all Visual C++ versions). */
  ids: string[];
  /** Not in winget: open this official download page instead (must be listed in tools.rs SITES). */
  url?: string;
  /** Other names the program uses in Windows' installed list. */
  match?: string[];
  /** Trust winget's list instead of Windows' installed list (no usable entry there). */
  byWinget?: boolean;
  ru: string;
  en: string;
  /** Site the icon was taken from. */
  site: string;
}

const VC = ["2015+", "2013", "2012", "2010", "2008"].flatMap((v) => [`Microsoft.VCRedist.${v}.x64`, `Microsoft.VCRedist.${v}.x86`]);

export const CATALOG: CatalogApp[] = [
  { key: "chrome", name: "Google Chrome", cat: "browsers", ids: ["Google.Chrome"], ru: "Браузер от Google", en: "Google's browser", site: "google.com" },
  { key: "firefox", name: "Firefox", cat: "browsers", ids: ["Mozilla.Firefox"], ru: "Свободный браузер", en: "Open-source browser", site: "mozilla.org" },
  { key: "operagx", name: "Opera GX", cat: "browsers", ids: ["Opera.OperaGX"], ru: "Браузер для геймеров", en: "Browser for gamers", site: "opera.com" },
  { key: "brave", name: "Brave", cat: "browsers", ids: ["Brave.Brave"], ru: "Браузер с блокировкой рекламы", en: "Browser with an ad blocker", site: "brave.com" },

  { key: "discord", name: "Discord", cat: "chat", ids: ["Discord.Discord"], ru: "Голос и чаты для игр", en: "Voice and chat for gaming", site: "discord.com" },
  { key: "telegram", name: "Telegram", cat: "chat", ids: ["Telegram.TelegramDesktop"], ru: "Мессенджер", en: "Messenger", site: "telegram.org" },
  { key: "zoom", name: "Zoom", cat: "chat", ids: ["Zoom.Zoom"], ru: "Видеозвонки", en: "Video calls", site: "zoom.us" },
  { key: "viber", name: "Viber", cat: "chat", ids: ["Rakuten.Viber"], ru: "Мессенджер и звонки", en: "Messenger and calls", site: "viber.com" },
  { key: "signal", name: "Signal", cat: "chat", ids: ["OpenWhisperSystems.Signal"], ru: "Защищённый мессенджер", en: "Private messenger", site: "signal.org" },
  { key: "whatsapp", name: "WhatsApp", cat: "chat", ids: ["9NKSQGP7F2NH"], ru: "Мессенджер", en: "Messenger", site: "whatsapp.com" },

  { key: "steam", name: "Steam", cat: "games", ids: ["Valve.Steam"], ru: "Магазин игр", en: "Game store", site: "store.steampowered.com" },
  { key: "epic", name: "Epic Games", cat: "games", ids: ["EpicGames.EpicGamesLauncher"], ru: "Магазин игр и бесплатные раздачи", en: "Game store with free games", site: "epicgames.com" },
  { key: "ea", name: "EA app", cat: "games", ids: ["ElectronicArts.EADesktop"], ru: "Игры Electronic Arts", en: "Electronic Arts games", site: "ea.com" },
  { key: "ubisoft", name: "Ubisoft Connect", cat: "games", ids: ["Ubisoft.Connect"], ru: "Игры Ubisoft", en: "Ubisoft games", site: "ubisoft.com" },
  { key: "battlenet", name: "Battle.net", cat: "games", ids: ["Blizzard.BattleNet"], ru: "Игры Blizzard", en: "Blizzard games", site: "battle.net" },
  { key: "gog", name: "GOG Galaxy", cat: "games", ids: ["GOG.Galaxy"], ru: "Игры без DRM", en: "DRM-free games", site: "gog.com" },
  { key: "roblox", name: "Roblox", cat: "games", ids: ["Roblox.Roblox"], ru: "Игровая платформа", en: "Game platform", site: "roblox.com" },
  { key: "rockstar", name: "Rockstar Games", cat: "games", ids: ["RockstarGames.Launcher"], ru: "GTA, Red Dead Redemption", en: "GTA, Red Dead Redemption", site: "rockstargames.com" },
  { key: "radmin", name: "Radmin VPN", cat: "games", ids: ["Famatech.RadminVPN"], ru: "Играть с друзьями по «локалке»", en: "Play with friends over a virtual LAN", site: "radmin-vpn.com" },

  { key: "nvidia", name: "NVIDIA App", cat: "drivers", ids: ["XP8CLZL93F5Z4P"], ru: "Драйверы NVIDIA", en: "NVIDIA drivers", site: "nvidia.com" },
  { key: "amd", name: "AMD Adrenalin", cat: "drivers", ids: [], url: "https://www.amd.com/ru/support/download/drivers.html", ru: "Драйверы для видеокарт AMD", en: "Drivers for AMD graphics", site: "amd.com" },

  { key: "vcredist", name: "Visual C++ (все версии)", cat: "components", ids: VC, ru: "Без них часто не запускаются игры", en: "Many games won't start without them", site: "visualstudio.microsoft.com" },
  { key: "dotnet", name: ".NET Desktop Runtime", cat: "components", ids: ["Microsoft.DotNet.DesktopRuntime.8", "Microsoft.DotNet.DesktopRuntime.9"], ru: "Нужен многим программам", en: "Needed by many programs", site: "dotnet.microsoft.com" },
  { key: "directx", name: "DirectX", cat: "components", ids: ["Microsoft.DirectX"], ru: "Старые библиотеки для игр", en: "Legacy libraries for games", site: "microsoft.com" },
  { key: "java", name: "Java", cat: "components", ids: ["Oracle.JavaRuntimeEnvironment"], ru: "Для Minecraft Java и программ на Java", en: "For Minecraft Java and Java apps", site: "java.com" },

  { key: "vlc", name: "VLC", cat: "media", ids: ["VideoLAN.VLC"], ru: "Плеер, который открывает всё", en: "Plays everything", site: "videolan.org" },
  { key: "obs", name: "OBS Studio", cat: "media", ids: ["OBSProject.OBSStudio"], ru: "Запись экрана и стримы", en: "Screen recording and streaming", site: "obsproject.com" },
  { key: "audacity", name: "Audacity", cat: "media", ids: ["Audacity.Audacity"], ru: "Запись и обработка звука", en: "Audio recording and editing", site: "audacityteam.org" },
  { key: "klite", name: "K-Lite Codec Pack", cat: "media", ids: ["CodecGuide.K-LiteCodecPack.Standard"], ru: "Кодеки для любых видео", en: "Codecs for any video", site: "codecguide.com" },
  { key: "handbrake", name: "HandBrake", cat: "media", ids: ["HandBrake.HandBrake"], ru: "Сжатие и конвертация видео", en: "Video compression and conversion", site: "handbrake.fr" },
  { key: "aimp", name: "AIMP", cat: "media", ids: ["AIMP.AIMP"], ru: "Музыкальный плеер", en: "Music player", site: "aimp.ru" },
  { key: "soundcloud", name: "SoundCloud", cat: "media", ids: ["9N5QR3KW6CMC"], ru: "Музыка и подкасты", en: "Music and podcasts", site: "soundcloud.com" },
  { key: "spotify", name: "Spotify", cat: "media", ids: ["9NCBCSZSJRSB"], ru: "Музыка онлайн", en: "Music streaming", site: "spotify.com" },

  { key: "gimp", name: "GIMP", cat: "graphics", ids: ["GIMP.GIMP.3"], ru: "Редактор фото", en: "Photo editor", site: "gimp.org" },
  { key: "paintnet", name: "Paint.NET", cat: "graphics", ids: ["dotPDN.PaintDotNet"], ru: "Простой редактор картинок", en: "Simple image editor", site: "getpaint.net" },
  { key: "krita", name: "Krita", cat: "graphics", ids: ["KDE.Krita"], ru: "Рисование", en: "Digital painting", site: "krita.org" },
  { key: "blender", name: "Blender", cat: "graphics", ids: ["BlenderFoundation.Blender"], ru: "3D-графика и анимация", en: "3D graphics and animation", site: "blender.org" },
  { key: "sharex", name: "ShareX", cat: "graphics", ids: ["ShareX.ShareX"], ru: "Скриншоты и запись экрана", en: "Screenshots and screen capture", site: "getsharex.com" },
  { key: "lightshot", name: "Lightshot", cat: "graphics", ids: ["Skillbrains.Lightshot"], ru: "Быстрые скриншоты", en: "Quick screenshots", site: "app.prntscr.com" },

  { key: "libreoffice", name: "LibreOffice", cat: "docs", ids: ["TheDocumentFoundation.LibreOffice"], ru: "Бесплатный офис: документы, таблицы", en: "Free office suite", site: "libreoffice.org" },
  { key: "acrobat", name: "Adobe Acrobat Reader", cat: "docs", ids: ["Adobe.Acrobat.Reader.64-bit"], match: ["Adobe Acrobat"], ru: "Просмотр PDF", en: "PDF viewer", site: "adobe.com" },
  { key: "sumatra", name: "SumatraPDF", cat: "docs", ids: ["SumatraPDF.SumatraPDF"], ru: "Лёгкая читалка PDF и книг", en: "Lightweight PDF and e-book reader", site: "sumatrapdfreader.org" },
  { key: "obsidian", name: "Obsidian", cat: "docs", ids: ["Obsidian.Obsidian"], ru: "Заметки и база знаний", en: "Notes and knowledge base", site: "obsidian.md" },
  { key: "notion", name: "Notion", cat: "docs", ids: ["Notion.Notion"], ru: "Заметки, задачи, вики", en: "Notes, tasks, wiki", site: "notion.so" },

  { key: "7zip", name: "7-Zip", cat: "utils", ids: ["7zip.7zip"], ru: "Архиватор", en: "Archiver", site: "7-zip.org" },
  { key: "winrar", name: "WinRAR", cat: "utils", ids: ["RARLab.WinRAR"], ru: "Архиватор", en: "Archiver", site: "win-rar.com" },
  { key: "everything", name: "Everything", cat: "utils", ids: ["voidtools.Everything"], ru: "Мгновенный поиск файлов", en: "Instant file search", site: "voidtools.com" },
  { key: "powertoys", name: "PowerToys", cat: "utils", ids: ["Microsoft.PowerToys"], ru: "Полезные дополнения к Windows", en: "Handy Windows add-ons", site: "learn.microsoft.com" },
  { key: "notepadpp", name: "Notepad++", cat: "utils", ids: ["Notepad++.Notepad++"], ru: "Удобный блокнот", en: "Better notepad", site: "notepad-plus-plus.org" },
  { key: "qbittorrent", name: "qBittorrent", cat: "utils", ids: ["qBittorrent.qBittorrent"], ru: "Торрент-клиент без рекламы", en: "Ad-free torrent client", site: "qbittorrent.org" },
  { key: "anydesk", name: "AnyDesk", cat: "utils", ids: ["AnyDesk.AnyDesk"], ru: "Удалённый доступ к ПК", en: "Remote desktop", site: "anydesk.com" },
  { key: "autohotkey", name: "AutoHotkey", cat: "utils", ids: ["AutoHotkey.AutoHotkey"], ru: "Свои горячие клавиши и макросы", en: "Custom hotkeys and macros", site: "autohotkey.com" },

  { key: "cpuz", name: "CPU-Z", cat: "hardware", ids: ["CPUID.CPU-Z"], ru: "Сведения о процессоре и памяти", en: "CPU and memory info", site: "cpuid.com" },
  { key: "gpuz", name: "GPU-Z", cat: "hardware", ids: ["TechPowerUp.GPU-Z"], ru: "Сведения о видеокарте", en: "Graphics card info", site: "techpowerup.com" },
  { key: "hwinfo", name: "HWiNFO", cat: "hardware", ids: ["REALiX.HWiNFO"], match: ["HWiNFO64"], ru: "Температуры и датчики", en: "Temperatures and sensors", site: "hwinfo.com" },
  { key: "crystaldiskinfo", name: "CrystalDiskInfo", cat: "hardware", ids: ["CrystalDewWorld.CrystalDiskInfo"], ru: "Здоровье дисков", en: "Drive health", site: "crystalmark.info" },
  { key: "afterburner", name: "MSI Afterburner", cat: "hardware", ids: ["Guru3D.Afterburner"], ru: "FPS в играх и разгон видеокарты", en: "In-game FPS and GPU tuning", site: "msi.com" },

  { key: "vscode", name: "VS Code", cat: "dev", ids: ["Microsoft.VisualStudioCode"], match: ["Visual Studio Code"], ru: "Редактор кода", en: "Code editor", site: "code.visualstudio.com" },
  { key: "git", name: "Git", cat: "dev", ids: ["Git.Git"], ru: "Контроль версий", en: "Version control", site: "git-scm.com" },
  { key: "python", name: "Python", cat: "dev", ids: ["Python.Python.3.13"], ru: "Язык программирования", en: "Programming language", site: "python.org" },
  { key: "nodejs", name: "Node.js", cat: "dev", ids: ["OpenJS.NodeJS.LTS"], ru: "JavaScript вне браузера", en: "JavaScript runtime", site: "nodejs.org" },
  { key: "vs", name: "Visual Studio Community", cat: "dev", ids: ["Microsoft.VisualStudio.2022.Community"], ru: "Среда разработки от Microsoft", en: "Microsoft's IDE", site: "visualstudio.microsoft.com" },
  { key: "githubdesktop", name: "GitHub Desktop", cat: "dev", ids: ["GitHub.GitHubDesktop"], ru: "Git с понятным окном", en: "Git with a friendly window", site: "desktop.github.com" },
  { key: "terminal", name: "Windows Terminal", cat: "dev", ids: ["Microsoft.WindowsTerminal"], byWinget: true, ru: "Современная командная строка", en: "Modern command line", site: "microsoft.com" },
  { key: "docker", name: "Docker Desktop", cat: "dev", ids: ["Docker.DockerDesktop"], ru: "Контейнеры", en: "Containers", site: "docker.com" },
  { key: "postman", name: "Postman", cat: "dev", ids: ["Postman.Postman"], ru: "Проверка API", en: "API testing", site: "postman.com" },
];

export const CATEGORIES: CatalogCategory[] = ["browsers", "chat", "games", "drivers", "components", "media", "graphics", "docs", "utils", "hardware", "dev"];

