import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import SettingsPage from "./pages/Settings";
import OverlayPage from "./pages/Overlay";

const label = getCurrentWebviewWindow().label;

export default function App() {
  if (label === "overlay") {
    return <OverlayPage />;
  }
  return <SettingsPage />;
}
