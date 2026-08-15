// Regenerate the Ocean theme's inlined ambient asset from its readable SVG
// source (themes/ocean/assets/whale.svg). Keeps the checked-in theme.json in
// sync without hand-editing base64.
//
// Usage: node scripts/gen_theme_assets.js
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const themePath = path.join(root, "themes", "ocean", "theme.json");
const svgPath = path.join(root, "themes", "ocean", "assets", "whale.svg");

const svg = fs.readFileSync(svgPath);
const dataUri = `data:image/svg+xml;base64,${svg.toString("base64")}`;

const theme = JSON.parse(fs.readFileSync(themePath, "utf8"));
theme.asset = dataUri;
fs.writeFileSync(themePath, `${JSON.stringify(theme, null, 2)}\n`);
console.log(`Updated ${path.relative(root, themePath)} (asset = ${dataUri.length} chars)`);
