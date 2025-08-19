const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function generateBuildInfo() {
  try {
    // Get the latest git tag
    let tag;
    try {
      tag = execSync('git describe --tags --abbrev=0', { encoding: 'utf-8' }).trim();
    } catch (e) {
      // If no tags exist, use commit hash only
      tag = null;
    }
    
    // Get the short commit hash
    const commit = execSync('git rev-parse --short HEAD', { encoding: 'utf-8' }).trim();
    
    // Get the build timestamp
    const timestamp = new Date().toISOString();
    
    const buildInfo = {
      tag,
      commit,
      timestamp,
      version: tag || commit
    };
    
    // Write to src/lib/build-info.json
    const outputPath = path.join(__dirname, '..', 'src', 'lib', 'build-info.json');
    fs.writeFileSync(outputPath, JSON.stringify(buildInfo, null, 2));
    
    console.log('Build info generated:', buildInfo);
  } catch (error) {
    console.error('Failed to generate build info:', error.message);
    
    // Create fallback build info
    const fallbackInfo = {
      tag: null,
      commit: 'unknown',
      timestamp: new Date().toISOString(),
      version: 'dev'
    };
    
    const outputPath = path.join(__dirname, '..', 'src', 'lib', 'build-info.json');
    fs.writeFileSync(outputPath, JSON.stringify(fallbackInfo, null, 2));
  }
}

// Run if called directly
if (require.main === module) {
  generateBuildInfo();
}

module.exports = generateBuildInfo;