import { ref } from "vue";
import type { ConfirmChoice, ConfirmDialogOptions } from "@app/types";

export function useAppConfirmDialog() {
  const confirmDialog = ref<ConfirmDialogOptions & { visible: boolean }>({
    visible: false,
    title: "",
    message: "",
    primaryLabel: "",
    secondaryLabel: "",
    danger: false,
  });

  let confirmDialogResolve: ((choice: ConfirmChoice) => void) | null = null;

  function requestConfirm(options: ConfirmDialogOptions): Promise<ConfirmChoice> {
    confirmDialog.value = {
      visible: true,
      danger: false,
      ...options,
    };

    return new Promise((resolve) => {
      confirmDialogResolve = resolve;
    });
  }

  function resolveConfirm(choice: ConfirmChoice) {
    confirmDialog.value.visible = false;
    confirmDialogResolve?.(choice);
    confirmDialogResolve = null;
  }

  return {
    confirmDialog,
    requestConfirm,
    resolveConfirm,
  };
}
