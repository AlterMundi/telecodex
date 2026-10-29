#!/usr/bin/env bash
set -euo pipefail
mkdir -p decoder-evidence
decoder_emulator_pid=''
decoder_user_home=$(mktemp -d "${RUNNER_TEMP:-/tmp}/interaction-decoder.XXXXXX")
decoder_avd_home="$decoder_user_home/avd"
mkdir -p "$decoder_avd_home"
cleanup() {
  if [ -n "$decoder_emulator_pid" ]; then
    kill "$decoder_emulator_pid" 2>/dev/null || true
    wait "$decoder_emulator_pid" 2>/dev/null || true
  fi
  rm -rf -- "$decoder_user_home"
  for log in decoder-evidence/*.log; do
    [ -f "$log" ] || continue
    tail -c 524288 "$log" >"$log.bounded"
    mv "$log.bounded" "$log"
  done
}
trap cleanup EXIT
test "$(df --output=avail -k . | tail -1)" -gt 10000000
timeout 600s "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" 'platform-tools' 'system-images;android-35;google_apis;x86_64' >decoder-evidence/sdk.log 2>&1
echo no | env ANDROID_USER_HOME="$decoder_user_home" ANDROID_AVD_HOME="$decoder_avd_home" "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" create avd --name interaction-decoder --path "$decoder_avd_home/interaction-decoder.avd" --package 'system-images;android-35;google_apis;x86_64' --force
test -f "$decoder_avd_home/interaction-decoder.ini"
sudo chmod a+rw /dev/kvm
env ANDROID_USER_HOME="$decoder_user_home" ANDROID_AVD_HOME="$decoder_avd_home" "$ANDROID_HOME/emulator/emulator" -avd interaction-decoder -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect -memory 1536 -cores 2 -no-snapshot -camera-back none -camera-front none >decoder-evidence/emulator.log 2>&1 &
decoder_emulator_pid=$!
decoder_adb="$ANDROID_HOME/platform-tools/adb"
test -x "$decoder_adb"
timeout 120s "$decoder_adb" wait-for-device
for attempt in $(seq 1 60); do
  kill -0 "$decoder_emulator_pid"
  if [ "$("$decoder_adb" shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; then break; fi
  sleep 2
done
test "$("$decoder_adb" shell getprop sys.boot_completed | tr -d '\r')" = 1
"$decoder_adb" shell input keyevent 82
"$decoder_adb" install -t "$1/debug/app-debug.apk"
"$decoder_adb" install -t "$1/androidTest/debug/app-debug-androidTest.apk"
timeout 110s "$decoder_adb" shell am instrument -w -r -e class homes.dmn.interaction.ProgressiveSpeechTest,homes.dmn.interaction.BackgroundSpeechTest homes.dmn.interaction.pilot.test/androidx.test.runner.AndroidJUnitRunner | tee decoder-evidence/test.log
"$decoder_adb" logcat -d -t 2000 >decoder-evidence/logcat.log
grep -q 'OK (2 tests)' decoder-evidence/test.log
test "$(du -sk decoder-evidence | cut -f1)" -lt 2048
