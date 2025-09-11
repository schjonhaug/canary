const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function generateBuildInfo() {
  const outputPath = path.join(__dirname, '..', 'src', 'lib', 'build-info.json');
  
  try {
    // Check if we're in a git repository
    execSync('git rev-parse --git-dir', { encoding: 'utf-8' });
    
    let tag = null;
    let commit = null;
    
    // Try to get tag
    try {
      tag = execSync('git describe --tags --abbrev=0', { encoding: 'utf-8' }).trim();
    } catch (e) {
      // No tags found, that's okay
    }
    
    // Get commit hash
    commit = execSync('git rev-parse --short HEAD', { encoding: 'utf-8' }).trim();
    
    const buildInfo = {
      tag,
      commit,
      timestamp: new Date().toISOString(),
      version: tag || commit
    };
    
    // Ensure the directory exists
    const libDir = path.join(__dirname, '..', 'src', 'lib');
    if (!fs.existsSync(libDir)) {
      fs.mkdirSync(libDir, { recursive: true });
    }
    
    // Write to src/lib/build-info.json
    fs.writeFileSync(outputPath, JSON.stringify(buildInfo, null, 2));
    
    console.log('Build info generated:', buildInfo);
  } catch (error) {
    // Not in a git repository - don't generate file
    console.log('Not in a git repository, skipping build info generation');
    
    // Remove file if it exists (clean state for Umbrel)
    if (fs.existsSync(outputPath)) {
      fs.unlinkSync(outputPath);
      console.log('Removed existing build-info.json');
    }
  }
}

// Run if called directly
if (require.main === module) {
  generateBuildInfo();
}

module.exports = generateBuildInfo;