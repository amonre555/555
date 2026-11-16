// Choix de la sortie audio (haut-parleurs, casque, Bluetooth déjà jumelé, AirPlay…), comme le
// sélecteur d'appareils de Spotify. Ordinateur seulement : sur téléphone, le système s'en charge.
import { createResource, For, Show } from "solid-js";
import { api, type OutputDevice } from "./api";
import { IconSpeaker } from "./icons";
import { t as tr } from "./i18n";
import { isAndroid } from "./mobile";
import { notify } from "./collection";
import { Popover } from "./queue";

/** Android et iOS : une seule sortie, choisie par le système. */
const phone = isAndroid || /iPhone|iPad|iPod/.test(navigator.userAgent) || (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1);

export function DeviceButton() {
  return (
    <Show when={!phone}>
      <Popover icon={<IconSpeaker />} title={tr.outputDevice} class="device-popover">
        {(close) => <DevicePanel close={close} />}
      </Popover>
    </Show>
  );
}

const KIND_LABEL: Record<string, () => string> = {
  builtIn: () => tr.deviceBuiltIn,
  bluetooth: () => tr.deviceBluetooth,
  usb: () => tr.deviceUsb,
  hdmi: () => tr.deviceHdmi,
  network: () => tr.deviceNetwork,
};

function DevicePanel(props: { close: () => void }) {
  // Relu à chaque ouverture : un casque vient peut-être d'être branché.
  const [list, { mutate }] = createResource(() => api.outputDevices().catch(() => null));
  const pick = async (id: string | null) => {
    try {
      mutate(await api.setOutputDevice(id));
      props.close();
    } catch (e) {
      notify(tr.deviceError(String(e)));
    }
  };
  const isOn = (d: OutputDevice) => list()?.active === d.id;
  return (
    <>
      <div class="eq-head">
        <span class="eq-title">{tr.outputDevice}</span>
      </div>
      <Show when={list()} fallback={<p class="queue-empty">{list.loading ? "…" : tr.deviceNone}</p>}>
        {(l) => (
          <div class="queue-scroll">
            <button classList={{ "menu-item": true, "device-on": l().selected == null }} role="menuitemradio" aria-checked={l().selected == null} onClick={() => pick(null)}>
              <IconSpeaker />
              <span class="menu-text">
                {tr.deviceSystem}
                <span class="device-sub">{l().devices.find((d) => d.isDefault)?.name ?? ""}</span>
              </span>
            </button>
            <div class="menu-label">{tr.devices}</div>
            <For each={l().devices}>
              {(d) => (
                <button
                  classList={{ "menu-item": true, "device-on": isOn(d) }}
                  role="menuitemradio"
                  aria-checked={l().selected === d.id}
                  onClick={() => pick(d.id)}
                >
                  <IconSpeaker />
                  <span class="menu-text">
                    {d.name}
                    <Show when={KIND_LABEL[d.kind] || isOn(d)}>
                      <span class="device-sub">
                        {[KIND_LABEL[d.kind]?.(), isOn(d) ? tr.devicePlaying : null].filter(Boolean).join(" · ")}
                      </span>
                    </Show>
                  </span>
                </button>
              )}
            </For>
          </div>
        )}
      </Show>
    </>
  );
}
