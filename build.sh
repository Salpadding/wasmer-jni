#!/usr/bin/env bash
CUR=$(dirname $0)
CUR=$(cd ${CUR}; pwd)
pushd $CUR >>/dev/null
pushd rust >>/dev/null
cargo build --release

popd >>/dev/null

# run JNIUtil to get OS and destination 
pushd java/src/main/java

javac com/github/salpadding/wasmer/JNIUtil.java
export LIB_FILE=`java com.github.salpadding.wasmer.JNIUtil`
export OS=`java com.github.salpadding.wasmer.JNIUtil OS`
rm -rf  com/github/salpadding/wasmer/*.class
export LIB_FILE=java/src/main/resources$LIB_FILE

echo "LIB_FILE=$LIB_FILE"

export LIB_DIR=`dirname $LIB_FILE`

echo "LIB_DIR=$LIB_DIR"
echo "OS=$OS"
popd >>/dev/null

mkdir -p $LIB_DIR
export LIB_BASE_NAME=`basename $LIB_FILE`

echo "BAENAME=$LIB_BASE_NAME"

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