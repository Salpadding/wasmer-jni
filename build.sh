#!/usr/bin/env bash
CUR=$(dirname $0)
CUR=$(cd ${CUR}; pwd)
pushd $CUR >>/dev/null
pushd rust >>/dev/null
cargo build --release

SYS=""

if [[ "$OSTYPE" == "linux-gnu"* ]]; then
  SYS="linux"
elif [[ "$OSTYPE" == "darwin"* ]]; then
  SYS="osx"
elif [[ "$OSTYPE" == "cygwin" ]]; then
    echo "invalid os type $OSTYPE" 1>&2
    exit 1
elif [[ "$OSTYPE" == "msys" ]]; then
    echo "invalid os type $OSTYPE" 1>&2
    exit 1
elif [[ "$OSTYPE" == "win32" ]]; then
  SYS="windows"
elif [[ "$OSTYPE" == "freebsd"* ]]; then
    echo "invalid os type $OSTYPE" 1>&2
    exit 1
else
    echo "invalid os type $OSTYPE" 1>&2
    exit 1
fi


DIR=../java/src/main/resources/lib/"${SYS}"/$(uname -m)
mkdir -p "${DIR}"

if [[ $SYS == "linux" ]]; then
  cp target/release/*.so $DIR
elif [[ $SYS == "windows" ]]; then
  cp target/release/*.dll $DIR
elif [[ $SYS == "osx" ]]; then
  cp target/release/*.dylib $DIR
else
    echo "invalid os type $SYS" 1>&2
    exit 1
fi

popd>>/dev/null

pushd java>>/dev/null
./gradlew jar

popd>>/dev/null