import { INITIAL_DIALOGS, type DialogName, type DialogState } from "./types";

/** Opens or closes one dialog without touching the others. */
export function setDialogOpen(
  dialogs: DialogState,
  name: DialogName,
  open: boolean,
): DialogState {
  if (dialogs[name] === open) {
    return dialogs;
  }
  return { ...dialogs, [name]: open };
}

/** Closes every overlay. */
export function closeAllDialogs(): DialogState {
  return INITIAL_DIALOGS;
}
