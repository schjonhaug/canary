import { chromium } from "@playwright/test"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const scriptDir = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(scriptDir, "../..")
const outputDir =
  process.env.UMBREL_GALLERY_OUTPUT_DIR || path.join(repoRoot, "screenshots/umbrel")

const cards = [
  {
    source: path.join(repoRoot, "screenshots/screenshot-01.png"),
    output: "1.jpg",
    title: "Import and monitor any Bitcoin wallet.",
    offsetY: 0,
  },
  {
    source: path.join(repoRoot, "screenshots/screenshot-03.png"),
    output: "2.jpg",
    title: "See every transaction as it happens.",
    offsetY: 0,
  },
  {
    source: path.join(repoRoot, "screenshots/screenshot-04.png"),
    output: "3.jpg",
    title: "Configure private notifications your way.",
    offsetY: -180,
  },
]

function imageDataUrl(filePath) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`Missing source screenshot: ${filePath}`)
  }
  return `data:image/png;base64,${fs.readFileSync(filePath).toString("base64")}`
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
}

function cardHtml(card) {
  const screenshot = imageDataUrl(card.source)
  return `<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <style>
      * { box-sizing: border-box; }
      html, body { width: 2160px; height: 1350px; margin: 0; overflow: hidden; }
      body {
        position: relative;
        color: white;
        font-family: Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        background:
          radial-gradient(circle at 12% 91%, rgba(255, 239, 0, .98) 0 12%, transparent 38%),
          radial-gradient(circle at 68% 62%, rgba(224, 211, 0, .72) 0 13%, transparent 42%),
          linear-gradient(123deg, #070700 0%, #151500 30%, #5d5a00 59%, #0a0a00 100%);
      }
      body::before {
        content: "";
        position: absolute;
        inset: -420px -300px;
        opacity: .32;
        transform: rotate(-9deg);
        background:
          repeating-conic-gradient(from 18deg at 48% 50%, rgba(255, 238, 0, .42) 0deg 13deg, transparent 13deg 31deg),
          repeating-linear-gradient(106deg, transparent 0 64px, rgba(255, 226, 0, .13) 65px 68px);
        filter: blur(1px);
      }
      body::after {
        content: "";
        position: absolute;
        inset: 0;
        opacity: .16;
        background-image: radial-gradient(rgba(255,255,255,.7) .8px, transparent .8px);
        background-size: 5px 5px;
        mix-blend-mode: overlay;
        pointer-events: none;
      }
      .title {
        position: absolute;
        z-index: 2;
        top: 82px;
        left: 160px;
        width: 1840px;
        margin: 0;
        text-align: center;
        font-size: 82px;
        line-height: 1.08;
        letter-spacing: -3.5px;
        font-weight: 800;
        text-wrap: balance;
        text-shadow: 0 3px 24px rgba(0, 0, 0, .4);
      }
      .browser {
        position: absolute;
        z-index: 2;
        top: 400px;
        left: 220px;
        width: 1720px;
        height: 1080px;
        overflow: hidden;
        border-radius: 20px 20px 0 0;
        background: #fff;
        box-shadow: 0 28px 80px rgba(0, 0, 0, .48), 0 0 0 1px rgba(255,255,255,.6);
      }
      .toolbar {
        position: relative;
        display: flex;
        align-items: center;
        height: 82px;
        padding: 0 30px;
        background: #f7f7f5;
        border-bottom: 1px solid #e7e7e4;
      }
      .dots { display: flex; gap: 14px; }
      .dot { width: 18px; height: 18px; border-radius: 50%; }
      .dot.red { background: #f45f57; }
      .dot.yellow { background: #f5bd3d; }
      .dot.green { background: #5ecb45; }
      .address {
        position: absolute;
        left: 50%;
        top: 16px;
        transform: translateX(-50%);
        width: 680px;
        height: 50px;
        border-radius: 12px;
        background: #ececea;
        color: #333;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 22px;
        font-weight: 600;
      }
      .viewport { height: 998px; overflow: hidden; background: white; }
      .viewport img {
        display: block;
        width: 100%;
        height: auto;
        transform: translateY(${card.offsetY}px);
      }
    </style>
  </head>
  <body>
    <h1 class="title">${escapeHtml(card.title)}</h1>
    <div class="browser">
      <div class="toolbar">
        <div class="dots">
          <span class="dot red"></span><span class="dot yellow"></span><span class="dot green"></span>
        </div>
        <div class="address">umbrel.local</div>
      </div>
      <div class="viewport"><img src="${screenshot}" alt=""></div>
    </div>
  </body>
</html>`
}

fs.mkdirSync(outputDir, { recursive: true })

const browser = await chromium.launch()
try {
  const page = await browser.newPage({ viewport: { width: 2160, height: 1350 } })
  for (const card of cards) {
    await page.setContent(cardHtml(card), { waitUntil: "load" })
    await page.screenshot({
      path: path.join(outputDir, card.output),
      type: "jpeg",
      quality: 92,
    })
    console.log(`Wrote Umbrel gallery ${card.output}`)
  }
} finally {
  await browser.close()
}
