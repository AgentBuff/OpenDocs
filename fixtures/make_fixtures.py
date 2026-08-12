#!/usr/bin/env python3
"""生成测试用 .docx 样本。

.docx 就是一个装着 XML 的 zip 包，所以这里不依赖 python-docx，直接手写最小的
ECMA-376 结构。这样 fixture 的每一个字节都是可控且可解释的，解析器测试断言什么
就一目了然。

用法：python3 fixtures/make_fixtures.py
"""

import zipfile
from pathlib import Path

OUT_DIR = Path(__file__).parent

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"""

# docDefaults 定义文档级默认值；Heading1 是一个具名段落样式，用来验证样式继承。
STYLES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults>
    <w:rPrDefault>
      <w:rPr>
        <w:rFonts w:ascii="Calibri"/>
        <w:sz w:val="22"/>
      </w:rPr>
    </w:rPrDefault>
    <w:pPrDefault>
      <w:pPr><w:spacing w:after="160" w:line="276" w:lineRule="auto"/></w:pPr>
    </w:pPrDefault>
  </w:docDefaults>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:pPr><w:jc w:val="center"/><w:spacing w:before="240" w:after="120"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="32"/><w:color w:val="2F5496"/></w:rPr>
  </w:style>
</w:styles>"""

LONG_CN = (
    "这是一段用于验证自动换行与分页的长文本。"
    "排版引擎需要在中文之间逐字断行，在西文单词之间按空格断行，"
    "并在正文区域高度用尽时开启新的一页。"
) * 6

LONG_EN = (
    "The layout engine must break Latin text at word boundaries rather than "
    "in the middle of a word, and it must fall back to breaking inside a word "
    "only when that single word is wider than the content box. "
) * 4

DOCUMENT = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>open-office 排版测试文档</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:t xml:space="preserve">普通文本，</w:t></w:r>
      <w:r><w:rPr><w:b/></w:rPr><w:t>加粗</w:t></w:r>
      <w:r><w:t xml:space="preserve">，</w:t></w:r>
      <w:r><w:rPr><w:i/></w:rPr><w:t>斜体</w:t></w:r>
      <w:r><w:t xml:space="preserve">，</w:t></w:r>
      <w:r><w:rPr><w:u w:val="single"/><w:color w:val="C00000"/></w:rPr><w:t>红色下划线</w:t></w:r>
      <w:r><w:t>。</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:jc w:val="right"/></w:pPr>
      <w:r><w:rPr><w:sz w:val="28"/></w:rPr><w:t>右对齐的 14 磅文本</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:ind w:firstLine="420"/></w:pPr>
      <w:r><w:t>{LONG_CN}</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:t>{LONG_EN}</w:t></w:r>
    </w:p>
    <w:p/>
    <w:p>
      <w:r><w:t>末段：空段落之后应当仍然正确排版。</w:t></w:r>
      <w:r><w:br/></w:r>
      <w:r><w:t>软换行之后的内容。</w:t></w:r>
    </w:p>
    <w:sectPr>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/>
    </w:sectPr>
  </w:body>
</w:document>"""

MINIMAL_DOCUMENT = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Hello, World!</w:t></w:r></w:p>
  </w:body>
</w:document>"""


def build(path: Path, document_xml: str) -> None:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/styles.xml", STYLES)
        z.writestr("word/document.xml", document_xml)
    print(f"已生成 {path} ({path.stat().st_size} 字节)")


if __name__ == "__main__":
    build(OUT_DIR / "sample.docx", DOCUMENT)
    build(OUT_DIR / "minimal.docx", MINIMAL_DOCUMENT)
