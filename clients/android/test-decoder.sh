#!/usr/bin/env bash
set -euo pipefail
mkdir -p decoder-evidence
test "$(df --output=avail -k . | tail -1)" -gt 10000000
timeout 600s "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" 'system-images;android-35;google_apis;x86_64' >decoder-evidence/sdk.log 2>&1
echo no | "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" create avd --name interaction-decoder --package 'system-images;android-35;google_apis;x86_64' --force
sudo chmod a+rw /dev/kvm
"$ANDROID_HOME/emulator/emulator" -avd interaction-decoder -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect -memory 1536 -cores 2 -no-snapshot -camera-back none -camera-front none >decoder-evidence/emulator.log 2>&1 &
decoder_emulator_pid=$!
trap 'kill "$decoder_emulator_pid" 2>/dev/null || true; wait "$decoder_emulator_pid" 2>/dev/null || true' EXIT
timeout 120s adb wait-for-device
for attempt in $(seq 1 60); do
  kill -0 "$decoder_emulator_pid"
  if [ "$(adb shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; then break; fi
  sleep 2
done
test "$(adb shell getprop sys.boot_completed | tr -d '\r')" = 1
adb shell input keyevent 82
adb install "$1/debug/app-debug.apk"
adb install "$1/androidTest/debug/app-debug-androidTest.apk"
timeout 75s adb shell am instrument -w -r -e class homes.dmn.interaction.ProgressiveSpeechTest homes.dmn.interaction.pilot.test/androidx.test.runner.AndroidJUnitRunner | tee decoder-evidence/test.log
adb logcat -d -t 2000 >decoder-evidence/logcat.log
grep -q 'OK (1 test)' decoder-evidence/test.log
test "$(du -sk decoder-evidence | cut -f1)" -lt 2048
