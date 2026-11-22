/** "Im Heimnetz freigeben" in the "Karten" panel: switch, port, address, QR code, firewall hint. Rendering only. */
import { api, type ShareStatus } from "./api";
import { t } from "./i18n";

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

export interface SharePanelHost {
  showMessage(text: string): void;
  showError(error: unknown): void;
}

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Without the clipboard API: copy a selection the old way.
    const area = document.createElement("textarea");
    area.value = text;
    document.body.append(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
}

export class SharePanel {
  private status: ShareStatus | null = null;
  private busy = false;

  constructor(private readonly host: SharePanelHost) {
    $("share-on").addEventListener("change", () => void this.toggle());
    $("share-address").addEventListener("change", () => void this.chooseAddress());
    $("share-url-copy").addEventListener("click", () => void this.copy("share-url"));
    $("share-firewall-copy").addEventListener("click", () => void this.copy("share-firewall"));
  }

  async refresh(): Promise<void> {
    try {
      this.status = await api.shareStatus();
    } catch (e) {
      this.host.showError(e);
    }
    this.render();
  }

  private async toggle(): Promise<void> {
    const on = $<HTMLInputElement>("share-on").checked;
    this.busy = true;
    this.render();
    try {
      if (on) {
        const port = Number($<HTMLInputElement>("share-port").value);
        if (!Number.isInteger(port) || port < 1 || port > 65535) {
          this.host.showMessage(t("share.port_invalid"));
        } else {
          this.status = await api.shareStart(port, null);
        }
      } else {
        await api.shareStop();
        this.status = await api.shareStatus();
      }
    } catch (e) {
      // A taken port: the switch stays off.
      this.host.showError(e);
      this.status = await api.shareStatus().catch(() => this.status);
    }
    this.busy = false;
    this.render();
  }

  private async chooseAddress(): Promise<void> {
    if (!this.status?.running) return;
    try {
      this.status = await api.shareStart(this.status.port, $<HTMLSelectElement>("share-address").value);
    } catch (e) {
      this.host.showError(e);
    }
    this.render();
  }

  private async copy(id: string): Promise<void> {
    await copy($(id).textContent ?? "");
    this.host.showMessage(t("share.copied"));
  }

  private render(): void {
    const status = this.status;
    const running = status?.running ?? false;
    const toggle = $<HTMLInputElement>("share-on");
    toggle.checked = running;
    toggle.disabled = status === null || this.busy;
    const port = $<HTMLInputElement>("share-port");
    port.disabled = running || this.busy;
    // Keep what the user typed while the share is off.
    if (status && (running || port.value === "")) port.value = String(status.port);

    const shown = running && status?.url != null;
    $("share-none").hidden = !running || shown;
    $("share-none").textContent = running && !shown ? t("share.no_address") : "";
    $("share-details").hidden = !shown;
    if (!status || !shown) return;

    const select = $<HTMLSelectElement>("share-address");
    select.replaceChildren(
      ...status.addresses.map((address) => {
        const option = document.createElement("option");
        option.value = address.ip;
        option.textContent = `${address.ip} (${address.interface})`;
        return option;
      }),
    );
    select.value = status.selected ?? "";
    $("share-address-field").hidden = status.addresses.length <= 1;
    $("share-url").textContent = status.url;
    // The image comes from the core (`qr_svg`), not from outside.
    $("share-qr").innerHTML = status.qr_svg ?? "";
    $("share-firewall").textContent = status.firewall_hint ?? "";
  }
}
