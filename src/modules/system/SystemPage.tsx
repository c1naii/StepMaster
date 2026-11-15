import { useState } from "react";
import { PageHeader, Tabs } from "../../components/ui";
import { useT } from "../../i18n";
import { useStore } from "../../lib/store";
import { Overview } from "./Overview";
import { Processes } from "./Processes";
import { Restore } from "./Restore";
import { Services } from "./Services";
import { Startup } from "./Startup";

type Tab = "overview" | "startup" | "processes" | "services" | "restore";
const TABS: Tab[] = ["overview", "startup", "processes", "services", "restore"];

export function SystemPage() {
  const t = useT();
  const [tab, setTab] = useState<Tab>(() => {
    const p = useStore.getState().takePendingTab();
    return TABS.includes(p as Tab) ? (p as Tab) : "overview";
  });

  return (
    <>
      <PageHeader title={t.system.title} sub={t.system.sub} />
      <Tabs
        value={tab}
        onChange={setTab}
        items={[
          { id: "overview", label: t.system.tabOverview },
          { id: "startup", label: t.system.tabStartup },
          { id: "processes", label: t.system.tabProcesses },
          { id: "services", label: t.system.tabServices },
          { id: "restore", label: t.system.tabRestore },
        ]}
      />
      {tab === "overview" ? <Overview /> : null}
      {tab === "startup" ? <Startup /> : null}
      {tab === "processes" ? <Processes /> : null}
      {tab === "services" ? <Services /> : null}
      {tab === "restore" ? <Restore /> : null}
    </>
  );
}
