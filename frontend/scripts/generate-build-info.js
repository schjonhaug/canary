const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function generateBuildInfo() {
  try {
    // Check if we're in a git repository
    execSync('git rev-parse --git-dir', { encoding: 'utf-8' });

    // Get commit hash
    const commit = execSync('git rev-parse --short HEAD', { encoding: 'utf-8' }).trim();

    console.log('Build commit:', commit);
    return commit;
  } catch (error) {
    // Not in a git repository
    console.log('Not in a git repository, skipping build info generation');
    return null;
  }
}

// Get the commit hash for use in next.config.ts
function getBuildCommit() {
  try {
    execSync('git rev-parse --git-dir', { encoding: 'utf-8' });
    return execSync('git rev-parse --short HEAD', { encoding: 'utf-8' }).trim();
  } catch {
    return null;
  }
}

// Run if called directly
if (require.main === module) {
  generateBuildInfo();
}

module.exports = { generateBuildInfo, getBuildCommit };
