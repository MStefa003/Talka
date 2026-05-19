/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        talka: {
          bg: "#0f0f13",
          card: "#1a1a24",
          border: "#2a2a3a",
          accent: "#6c63ff",
          red: "#ff4d4d",
          amber: "#ffaa00",
          green: "#4ade80",
        },
      },
    },
  },
  plugins: [],
};
