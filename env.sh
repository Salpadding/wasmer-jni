CUR=$(dirname $0)
CUR=$(cd ${CUR}; pwd)
pushd $CUR >>/dev/null

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
popd