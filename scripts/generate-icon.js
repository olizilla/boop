import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.join(__dirname, '..');

const svg = `
<svg width="1024" height="1024" viewBox="0 0 1024 1024" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:#1e1e24;stop-opacity:1" />
      <stop offset="100%" style="stop-color:#121214;stop-opacity:1" />
    </linearGradient>
  </defs>
  <rect width="100%" height="100%" rx="200" fill="url(#grad)"/>
  <text x="50%" y="55%" font-size="500" text-anchor="middle" dominant-baseline="middle" font-family="Apple Color Emoji, Segoe UI Emoji, Notation Color Emoji, Android Emoji, sans-serif">
    👉☎️👈
  </text>
</svg>
`;

const svgPath = path.join(rootDir, 'app_icon.svg');

fs.writeFileSync(svgPath, svg);

console.log(`Generated SVG icon at: ${svgPath}`);
