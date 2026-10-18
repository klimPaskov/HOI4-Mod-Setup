// An existing setup found in the dropped folder: what it is, and the
// desktop's maintenance choices, each producing a reviewed ZIP.

import type { ExistingSetup } from "../protocol";

export type MaintenanceChoice = "update" | "repair" | "change" | "remove";

interface Props {
  setup: ExistingSetup;
  choice: MaintenanceChoice | null;
  onChoose: (choice: MaintenanceChoice | null) => void;
}

const CHOICES: Array<{ value: MaintenanceChoice; label: string; detail: string }> = [
  { value: "update", label: "Update", detail: "Get the newest modding package. Files you never changed update; changed files are reviewed." },
  { value: "repair", label: "Repair", detail: "Restore missing or broken setup files at the installed version." },
  { value: "change", label: "Change clients or workflows", detail: "Add or remove coding clients, components, and integrations." },
  { value: "remove", label: "Remove the setup", detail: "List the setup files you never changed for deletion. Your mod files stay." },
];

export default function MaintenancePanel({ setup, choice, onChoose }: Props) {
  if (!setup.valid) {
    return (
      <section className="panel form-panel" aria-label="Existing setup">
        <p className="status-review"><span className="status-symbol" aria-hidden="true">!</span> {setup.reason ?? "The existing setup could not be read."} The folder is treated as a mod without a setup.</p>
      </section>
    );
  }
  const date = (value?: string | null) => (value ? value.slice(0, 10) : null);
  return (
    <section className="panel form-panel" aria-labelledby="maintenance-title">
      <h2 id="maintenance-title" className="panel-title">Existing setup found</h2>
      <p className="muted">
        Installed {date(setup.installed_at)}{setup.updated_at ? `, last changed ${date(setup.updated_at)}` : ""} from Agentic HOI4 Modding at <span className="mono">{setup.source?.revision.slice(0, 12)}</span>. {setup.file_count ?? 0} managed files.
      </p>
      <div className="web-radio-group" role="radiogroup" aria-label="What to do with the setup">
        <label className="radio-row">
          <input type="radio" name="maintenance" checked={choice === null} onChange={() => onChoose(null)} />
          <span><strong>Set up again</strong><span className="muted"> — ask the assistant again and review every file.</span></span>
        </label>
        {CHOICES.map((item) => (
          <label key={item.value} className="radio-row">
            <input type="radio" name="maintenance" checked={choice === item.value} onChange={() => onChoose(item.value)} />
            <span><strong>{item.label}</strong><span className="muted"> — {item.detail}</span></span>
          </label>
        ))}
      </div>
    </section>
  );
}
