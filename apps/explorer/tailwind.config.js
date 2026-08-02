/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        obsidian: "#101218",
        gold: "#C59835",
        cyan: "#06BBDF",
        ink: "#E8E6E1",
        mist: "#9AA0AB",
      },
      fontFamily: {
        display: ["Cinzel", "Times New Roman", "serif"],
        ui: ["Inter", "Segoe UI", "sans-serif"],
      },
    },
  },
  plugins: [],
};
