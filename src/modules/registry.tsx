// Module registry: every sidebar section lives in its own folder and is listed here.
// To add a new section: create src/modules/<id>/<Name>Page.tsx, add the id to ModuleId (lib/store.ts),
// add nav.<id> to i18n, and add one entry below.
import type { LucideIcon } from "lucide-react";
import { Cpu, Gamepad2, History, Image as Picture, HardDrive, House, LayoutGrid, Settings, Shield, Sparkles, Stethoscope, Wifi } from "lucide-react";
import type { ComponentType } from "react";
import type { ModuleId } from "../lib/store";
import { CleanupPage } from "./cleanup/CleanupPage";
import { DiagnosticsPage } from "./diagnostics/DiagnosticsPage";
import { DiskPage } from "./disk/DiskPage";
import { GamesPage } from "./games/GamesPage";
import { HomePage } from "./home/HomePage";
import { JournalPage } from "./journal/JournalPage";
import { NetworkPage } from "./network/NetworkPage";
import { ProgramsPage } from "./programs/ProgramsPage";
import { ProtectionPage } from "./protection/ProtectionPage";
import { SettingsPage } from "./settings/SettingsPage";
import { SystemPage } from "./system/SystemPage";
import { WallpapersPage } from "./wallpapers/WallpapersPage";

/** Sidebar groups (a thin line between them); "bottom" items sit at the foot of the menu. */
export type NavGroup = "start" | "care" | "system" | "personal" | "bottom";

export interface ModuleDef {
  id: ModuleId;
  icon: LucideIcon;
  component: ComponentType;
  group: NavGroup;
}

export const MODULES: ModuleDef[] = [
  { id: "home", icon: House, component: HomePage, group: "start" },
  { id: "diagnostics", icon: Stethoscope, component: DiagnosticsPage, group: "start" },
  { id: "cleanup", icon: Sparkles, component: CleanupPage, group: "care" },
  { id: "disk", icon: HardDrive, component: DiskPage, group: "care" },
  { id: "programs", icon: LayoutGrid, component: ProgramsPage, group: "care" },
  { id: "system", icon: Cpu, component: SystemPage, group: "system" },
  { id: "protection", icon: Shield, component: ProtectionPage, group: "system" },
  { id: "network", icon: Wifi, component: NetworkPage, group: "system" },
  { id: "games", icon: Gamepad2, component: GamesPage, group: "personal" },
  { id: "wallpapers", icon: Picture, component: WallpapersPage, group: "personal" },
  { id: "journal", icon: History, component: JournalPage, group: "bottom" },
  { id: "settings", icon: Settings, component: SettingsPage, group: "bottom" },
];
