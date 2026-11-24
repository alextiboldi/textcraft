#!/bin/sh
# Writes the encoding fixtures beside this script: one short text file per encoding, made with the
# system's iconv from sentences written for these tests (no third-party text). Each file is named
# after the encoding it was written in. `core/tests/it/main.rs` opens and saves every one and
# checks the bytes don't change, and uses them as seeds for its hostile-input sweep.
#
# Run from anywhere: sh core/tests/fixtures/encodings/generate.sh
set -eu
cd "$(dirname "$0")"

WEST='Le café du coin sert une crème brûlée à midi.
Die Brücke über den Fluß ist schön und breit.
El niño pidió más piñas para la mañana.
'
CENTRAL='Malý kůň běží přes zelenou louku.
Żółty pies śpi pod starą gruszą.
Kőműves Ödön ült a régi kút mellett.
'
CYRILLIC='Сегодня утром в городе шёл тёплый дождь.
Мы пили чай и читали старые письма у окна.
'
GREEK='Η θάλασσα ήταν ήσυχη και γαλάζια σήμερα.
Ο ψαράς γύρισε στο λιμάνι αργά το βράδυ.
'
TURKISH='Bugün çarşıda güzel bir şemsiye aldım.
Öğleden sonra ılık bir rüzgâr esiyordu.
'
BALTIC='Šodien Rīgā lija silts lietus.
Vakarą ėjome pasivaikščioti prie upės.
'
THAI='วันนี้อากาศดีมาก เราไปเดินเล่นที่สวนสาธารณะ
'
JAPANESE='今日は雨が降っていますが、明日は晴れるでしょう。
駅の近くに新しい本屋ができました。カタカナもひらがなも使います。
'
SIMPLIFIED='今天天气很好，我们去公园散步吧。
图书馆里有很多有趣的书，价格是一百元。
'
TRADITIONAL='今天天氣很好，我們去公園散步吧。
圖書館裡有很多有趣的書，價格是一百元。
'
KOREAN='오늘은 날씨가 맑고 바람이 시원합니다.
우리는 공원에서 함께 점심을 먹었습니다.
'

# name, iconv charset, text
write() { printf '%s' "$3" | iconv -f UTF-8 -t "$2" > "$1"; }

write utf-8.txt UTF-8 "$WEST$CYRILLIC$JAPANESE"
{ printf '\357\273\277'; printf '%s' "$WEST$GREEK"; } > utf-8-bom.txt
{ printf '\377\376'; printf '%s' "$WEST$KOREAN" | iconv -f UTF-8 -t UTF-16LE; } > utf-16le-bom.txt
{ printf '\376\377'; printf '%s' "$WEST$THAI" | iconv -f UTF-8 -t UTF-16BE; } > utf-16be-bom.txt

# Windows code pages
write windows-1250.txt WINDOWS-1250 "$CENTRAL"
write windows-1251.txt WINDOWS-1251 "$CYRILLIC"
write windows-1252.txt WINDOWS-1252 "$WEST"
write windows-1253.txt WINDOWS-1253 "$GREEK"
write windows-1254.txt WINDOWS-1254 "$TURKISH"
write windows-1257.txt WINDOWS-1257 "$BALTIC"
write windows-874.txt CP874 "$THAI"
# ISO 8859 and KOI8
write iso-8859-1.txt ISO-8859-1 "$WEST"
write iso-8859-2.txt ISO-8859-2 "$CENTRAL"
write iso-8859-5.txt ISO-8859-5 "$CYRILLIC"
write iso-8859-7.txt ISO-8859-7 "$GREEK"
write iso-8859-15.txt ISO-8859-15 "$WEST"
write koi8-r.txt KOI8-R "$CYRILLIC"
# Mac
write macintosh.txt MACINTOSH "$WEST"
write x-mac-cyrillic.txt MACCYRILLIC "$CYRILLIC"
# DOS code pages
write cp437.txt CP437 "$WEST"
write cp850.txt CP850 "$WEST"
write cp852.txt CP852 "$CENTRAL"
write ibm866.txt CP866 "$CYRILLIC"
# East Asian
write shift_jis.txt SHIFT_JIS "$JAPANESE"
write euc-jp.txt EUC-JP "$JAPANESE"
write iso-2022-jp.txt ISO-2022-JP "$JAPANESE"
write gbk.txt GBK "$SIMPLIFIED"
# A four-byte GB18030 sequence, which GBK lacks (U+3400, from CJK Extension A).
write gb18030.txt GB18030 "${SIMPLIFIED}扩展汉字：㐀
"
write big5.txt BIG5 "$TRADITIONAL"
write euc-kr.txt EUC-KR "$KOREAN"
