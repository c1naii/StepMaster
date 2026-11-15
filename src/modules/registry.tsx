// Module registry: every sidebar section lives in its own folder and is listed here.
// To add a new section: create src/modules/<id>/<Name>Page.tsx, add the id to ModuleId (lib/store.ts),
// add nav.<id> to i18n, and add one entry below.
import type { LucideIcon } from "lucide-react";
import { Cpu, Gamepad2, HardDrive, House, LayoutGrid, Settings, Shield, Sparkles, Stethoscope, Wifi } from "lucide-react";
import type { ComponentType } from "react";
import type { ModuleId } from "../lib/store";
import { CleanupPage } from "./cleanup/CleanupPage";
import { DiagnosticsPage } from "./diagnostics/DiagnosticsPage";
import { DiskPage } from "./disk/DiskPage";
import { GamesPage } from "./games/GamesPage";
import { HomePage } from "./home/HomePage";
import { NetworkPage } from "./network/NetworkPage";
import { ProgramsPage } from "./programs/ProgramsPage";
import { ProtectionPage } from "./protection/ProtectionPage";
import { SettingsPage } from "./settings/SettingsPage";
import { SystemPage } from "./system/SystemPage";

export interface ModuleDef {
  id: ModuleId;
  icon: LucideIcon;
  component: ComponentType;
}

export const MODULES: ModuleDef[] = [
  { id: "home", icon: House, component: HomePage },
  { id: "diagnostics", icon: Stethoscope, component: DiagnosticsPage },
  { id: "cleanup", icon: Sparkles, component: CleanupPage },
  { id: "disk", icon: HardDrive, component: DiskPage },
  { id: "programs", icon: LayoutGrid, component: ProgramsPage },
  { id: "system", icon: Cpu, component: SystemPage },
  { id: "protection", icon: Shield, component: ProtectionPage },
  { id: "network", icon: Wifi, component: NetworkPage },
  { id: "games", icon: Gamepad2, component: GamesPage },
  { id: "settings", icon: Settings, component: SettingsPage },
];
