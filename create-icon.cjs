const fs = require('fs');
const path = require('path');
const sharp = require('sharp');
const png2icons = require('png2icons');

// 解析命令行参数
const args = process.argv.slice(2);
let inputPath = path.join(__dirname, './app-icon.png'); // 默认输入路径
let icnsOutputPath = path.join(__dirname, './src-tauri/icons/icon.icns'); // 默认输出路径

// 处理命令行参数
for (let i = 0; i < args.length; i++) {
  if (args[i] === '-i' && i + 1 < args.length) {
    inputPath = path.resolve(process.cwd(), args[i + 1]);
  } else if (args[i] === '-o' && i + 1 < args.length) {
    icnsOutputPath = path.resolve(process.cwd(), args[i + 1]);
  }
}

// 验证输入文件是否存在
if (!fs.existsSync(inputPath)) {
  console.error(`Error: Input file not found at ${inputPath}`);
  process.exit(1);
}

// 确保输出目录存在
const outputDir = path.dirname(icnsOutputPath);
if (!fs.existsSync(outputDir)) {
  fs.mkdirSync(outputDir, { recursive: true });
}

console.log(`Using input file: ${inputPath}`);
console.log(`Output will be saved to: ${icnsOutputPath}`);

// 临时输出带圆角和 padding 的 PNG 文件路径
const processedOutputPath = path.join(__dirname, './processed-image.png');

// 添加圆角和 padding
sharp(inputPath)
  .resize({ width: 1024, height: 1024, fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
  .composite([
    {
      input: Buffer.from(
        `<svg><rect x="0" y="0" width="1024" height="1024" rx="250" ry="250" /></svg>`
      ),
      blend: 'dest-in',
    },
  ])
  .extend({
    top: 120,
    bottom: 120,
    left: 120,
    right: 120,
    background: { r: 0, g: 0, b: 0, alpha: 0 },
  })
  .toFile(processedOutputPath)
  .then(() => {
    console.log('Image processing complete with rounded corners and padding.');
    // 转换 PNG 到 ICNS 格式
    fs.readFile(processedOutputPath, (err, data) => {
      if (err) {
        console.error('Error reading processed PNG file:', err);
        return;
      }
      const icnsBuffer = png2icons.createICNS(data, 1, 0);
      if (icnsBuffer) {
        fs.writeFile(icnsOutputPath, icnsBuffer, (writeErr) => {
          if (writeErr) {
            console.error('Error writing ICNS file:', writeErr);
          } else {
            console.log('ICNS file created successfully.');
          }
        });
      }
    });
  });
