export function createPortInputProps() {
  let acceptedValue = "";
  const digitsOnly = (value: string) => /^[0-9]*$/.test(value);

  function rememberValue(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    if (digitsOnly(input.value)) acceptedValue = input.value;
  }

  function rejectInvalidValue(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    if (digitsOnly(input.value)) {
      acceptedValue = input.value;
    } else {
      input.value = acceptedValue;
    }
  }

  return {
    inputmode: "numeric" as const,
    pattern: "[0-9]*",
    onFocusCapture: rememberValue,
    onBeforeinput(event: Event) {
      rememberValue(event);
      const inputEvent = event as InputEvent;
      if (inputEvent.data && !digitsOnly(inputEvent.data)) event.preventDefault();
    },
    onPaste(event: ClipboardEvent) {
      rememberValue(event);
      if (!digitsOnly(event.clipboardData?.getData("text") ?? "")) event.preventDefault();
    },
    // Capture before NInput processes the value, including drop and IME input.
    onInputCapture(event: Event) {
      if (!(event as InputEvent).isComposing) rejectInvalidValue(event);
    },
    onCompositionendCapture: rejectInvalidValue,
  };
}
