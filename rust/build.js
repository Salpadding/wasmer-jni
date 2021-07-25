#!/usr/bin/env node

const child_process = require('child_process')
const os = require('os')
const fs = require('fs')
const path = require('path')

process.chdir(__dirname)

child_process.execSync('cargo build --release')


const osMap = {
    "win32": "windows",
    "windows": "windows",
    "linxu": "linux",
    "osx": "osx",
}

const archMap = {
    "x64": "amd64"
}

for(let f of fs.readdirSync('target/release')) {
    if (f.endsWith('.so') || f.endsWith('.dll') || f.endsWith('.dylib')) {
        const dest = `../java/src/main/resources/lib/${osMap[os.platform]}/${archMap[os.arch]}`
        console.log(dest)
        fs.copyFileSync(
            path.join('target/release', f), 
            dest
        )
    }
}