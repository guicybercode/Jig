import { open } from "@tauri-apps/plugin-dialog";

/** Opens the operating system folder browser and returns one local directory. */
export async function browseForProjectDirectory(): Promise<string | null> {
  const selection = await open({
    directory: true,
    multiple: false,
    title: "Choose a project folder",
  });

  return typeof selection === "string" ? selection : null;
}
