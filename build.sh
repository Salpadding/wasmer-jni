#!/usr/bin/env bash
CUR=$(dirname $0)
CUR=$(cd ${CUR}; pwd)
pushd $CUR >>/dev/null
pushd rust >>/dev/null
cargo build --release

popd >>/dev/null

source $CUR/env.sh

if [[ $OS == "linux" ]]; then
  cp rust/target/release/$LIB_BASE_NAME $LIB_FILE
elif [[ $OS == "windows" ]]; then
  cp rust/target/release/$LIB_BASE_NAME $LIB_FILE
elif [[ $OS == "osx" ]]; then
  cp rust/target/release/$LIB_BASE_NAME $LIB_FILE
else
    echo "invalid os type $SYS" 1>&2
    exit 1
fi


pushd java>>/dev/null
./gradlew test

popd>>/dev/null