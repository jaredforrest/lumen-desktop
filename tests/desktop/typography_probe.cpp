#include <QGuiApplication>
#include <QFont>
#include <QFontMetricsF>
#include <QGlyphRun>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPainter>
#include <QRawFont>
#include <QTextLayout>
#include <QFile>
#include <cstdio>

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    if (argc != 3) return 2;
    QFile input(QString::fromLocal8Bit(argv[1]));
    if (!input.open(QIODevice::ReadOnly)) return 3;
    const auto document=QJsonDocument::fromJson(input.readAll());
    if (!document.isObject()) return 4;
    const auto config=document.object();
    const auto cases=config.value("cases").toArray();
    const int scale=config.value("scale").toInt(1);
    if (cases.isEmpty() || scale<1 || scale>2) return 5;
    const int width=1400,height=60+cases.size()*76;
    QImage image(width*scale,height*scale,QImage::Format_ARGB32_Premultiplied);
    image.setDevicePixelRatio(scale);
    image.fill(QColor(config.value("background").toString()));
    QPainter painter(&image);
    painter.setRenderHints(QPainter::Antialiasing|QPainter::TextAntialiasing);
    QJsonArray result;
    for (qsizetype index=0; index<cases.size(); ++index) {
        const auto item=cases.at(index).toObject();
        const QString text=item.value("text").toString();
        QFont font(config.value("family").toString());
        font.setPixelSize(item.value("size").toInt(22));
        font.setStyleStrategy(QFont::PreferAntialias);
        if (item.value("tabular").toBool()) font.setFeature(*QFont::Tag::fromString("tnum"),1);
        QTextLayout layout(text,font);
        layout.beginLayout();
        auto line=layout.createLine();
        line.setLineWidth(width-48);
        layout.endLayout();
        QJsonArray runs;
        int missing=0;
        for (const auto &run:layout.glyphRuns(0,-1,QTextLayout::RetrieveAll)) {
            QJsonArray glyphs;
            for (quint32 glyph:run.glyphIndexes()) {glyphs.append(static_cast<qint64>(glyph));if (glyph==0)++missing;}
            runs.append(QJsonObject{{"family",run.rawFont().familyName()},{"glyphs",glyphs}});
        }
        QFont labelFont(font);labelFont.setPixelSize(12);
        painter.setFont(labelFont);painter.setPen(QColor(config.value("muted").toString()));
        painter.drawText(QPointF(24,30+index*76),item.value("name").toString());
        painter.setPen(QColor(config.value("foreground").toString()));
        layout.draw(&painter,QPointF(24,37+index*76));
        QJsonArray digitWidths;
        if (item.value("tabular").toBool()) {
            QFontMetricsF metrics(font);
            for (const QChar digit:QStringLiteral("0123456789")) digitWidths.append(metrics.horizontalAdvance(QString(digit)));
        }
        result.append(QJsonObject{{"name",item.value("name")},{"text",text},{"runs",runs},{"missingGlyphs",missing},{"width",line.naturalTextWidth()},{"height",line.height()},{"digitWidths",digitWidths}});
    }
    painter.end();
    if (!image.save(QString::fromLocal8Bit(argv[2]))) return 6;
    const auto output=QJsonDocument(QJsonObject{{"qtVersion",qVersion()},{"requestedFamily",config.value("family")},{"scale",scale},{"cases",result}}).toJson(QJsonDocument::Compact);
    std::fwrite(output.constData(),1,static_cast<size_t>(output.size()),stdout);
    return 0;
}
