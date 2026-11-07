#!/usr/bin/env bash
set -euo pipefail
test "$#" -ge 1 && test "$#" -le 2
ios_output="$1"
test ! -e "$ios_output"
ios_root=$(cd "$(dirname "$0")" && pwd)
ios_repo=$(cd "$ios_root/../.." && pwd)
mkdir -p "$ios_output/Daimon.app"
ios_app="$ios_output/Daimon.app"
ios_flags=()
ios_qualification_sources=()
ios_identifier='homes.dmn.interaction.pilot'
if [ "${2:-}" = '--qualification' ]; then
  ios_flags=(-D QUALIFICATION)
  ios_qualification_sources=("$ios_root/Qualification/FixtureTrust.swift")
  ios_identifier='homes.dmn.interaction.qualification'
else
  test "$#" -eq 1
fi
cp "$ios_repo/clients/android/app/src/main/assets/native-audio.js" "$ios_app/native-audio.js"
python3 - "$ios_app" "$ios_identifier" <<'PY'
import hashlib, plistlib, sys
from pathlib import Path
app=Path(sys.argv[1])
value={'CFBundleIdentifier':sys.argv[2],'CFBundleName':'Daimon','CFBundleDisplayName':'Daimon',
       'CFBundleExecutable':'Daimon','CFBundlePackageType':'APPL','CFBundleVersion':'1',
       'CFBundleShortVersionString':'0.1.0','MinimumOSVersion':'17.0',
       'CFBundleSupportedPlatforms':['iPhoneSimulator'],'UIDeviceFamily':[1,2],
       'UIApplicationSceneManifest':{'UIApplicationSupportsMultipleScenes':False},'UILaunchScreen':{},
       'UISupportedInterfaceOrientations':['UIInterfaceOrientationPortrait','UIInterfaceOrientationLandscapeLeft','UIInterfaceOrientationLandscapeRight'],
       'NSMicrophoneUsageDescription':'Record the voice messages you choose to send to your daimon.',
       'NativeAdapterSHA256':hashlib.sha256((app/'native-audio.js').read_bytes()).hexdigest()}
with (app/'Info.plist').open('wb') as f:plistlib.dump(value,f)
PY
xcrun --sdk iphonesimulator swiftc -swift-version 5 -parse-as-library \
  -sdk "$(xcrun --sdk iphonesimulator --show-sdk-path)" -target arm64-apple-ios17.0-simulator \
  -module-name DaimonApp ${ios_flags[@]+"${ios_flags[@]}"} "$ios_root"/Sources/DaimonNativeAudio/*.swift \
  "$ios_root/App/AppDelegate.swift" ${ios_qualification_sources[@]+"${ios_qualification_sources[@]}"} -o "$ios_app/Daimon"
codesign --force --sign - "$ios_app"
codesign --verify --strict "$ios_app"
python3 - "$ios_app" "$ios_repo" <<'PY'
from pathlib import Path
import sys
app,repo=map(Path,sys.argv[1:])
assert (app/'native-audio.js').read_bytes()==(repo/'clients/android/app/src/main/assets/native-audio.js').read_bytes()
assert sum(p.stat().st_size for p in app.rglob('*') if p.is_file())<16*1024*1024
print('Simulator app built; exact existing native adapter bundled; no device signing/distribution')
PY
