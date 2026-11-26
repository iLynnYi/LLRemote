import type { GlobalThemeOverrides } from "naive-ui";

export const naiveThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#1f8a70",
    primaryColorHover: "#176f5b",
    primaryColorPressed: "#125744",
    primaryColorSuppl: "#2aa985",
    borderRadius: "8px",
    fontFamily: 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  },
  Button: {
    fontWeight: "700",
  },
  Card: {
    borderRadius: "8px",
  },
  Input: {
    borderRadius: "7px",
  },
  Select: {
    peers: {
      InternalSelection: {
        borderRadius: "7px",
      },
    },
  },
};
