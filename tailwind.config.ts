import type { Config } from "tailwindcss";
import tailwindcssAnimate from "tailwindcss-animate";

export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "var(--bg)",
        ink: "var(--ink)",
        cyan: "var(--cyan)",
        green: "var(--green)",
        orange: "var(--orange)",
        violet: "var(--violet)",
      },
      fontFamily: {
        sans: ["var(--sans)"],
        mono: ["var(--mono)"],
      },
      borderRadius: {
        panel: "8px",
      },
      keyframes: {
        "wire-in": {
          "0%": { opacity: "0", transform: "translateY(16px) scale(0.98)" },
          "100%": { opacity: "1", transform: "translateY(0) scale(1)" },
        },
        flash: {
          "0%": { boxShadow: "0 0 0 rgba(0,246,255,0)" },
          "18%": { boxShadow: "0 0 20px rgba(0,246,255,0.3)" },
          "100%": { boxShadow: "0 0 0 rgba(0,246,255,0)" },
        },
        float: {
          "0%, 100%": { transform: "translateY(-3px)" },
          "50%": { transform: "translateY(3px)" },
        },
        ping: {
          "0%": { transform: "scale(1)", opacity: "0.86" },
          "100%": { transform: "scale(2.25)", opacity: "0" },
        },
        dash: {
          "0%": { strokeDashoffset: "36" },
          "100%": { strokeDashoffset: "0" },
        },
      },
      animation: {
        "wire-in": "wire-in 480ms cubic-bezier(0.2, 0.84, 0.22, 1) both",
        flash: "flash 1.2s ease both",
        float: "float 4s ease-in-out infinite",
        ping: "ping 1.5s ease-out infinite",
        dash: "dash 2.8s linear infinite",
      },
    },
  },
  plugins: [tailwindcssAnimate],
} satisfies Config;
