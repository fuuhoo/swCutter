import Cocoa
import FlutterMacOS

/// swCutter 主窗口：把默认尺寸设大（与 Windows 版 1280x720 对齐；macOS 略高一点以容纳 traffic light 与 title bar）。
class MainFlutterWindow: NSWindow {
  /// 默认窗口尺寸：宽 1280、高 800（比 Windows 720 多 80 留给 macOS title bar）
  static let defaultContentSize = NSSize(width: 1280, height: 800)
  /// 最小窗口尺寸：保证表单/预览双栏布局能渲染
  static let minContentSize = NSSize(width: 960, height: 640)

  override init(contentRect: NSRect, styleMask style: NSWindow.StyleMask, backing backingType: NSWindow.BackingStoreType, defer flag: Bool) {
    let rect = NSRect(
      x: contentRect.origin.x,
      y: contentRect.origin.y,
      width: MainFlutterWindow.defaultContentSize.width,
      height: MainFlutterWindow.defaultContentSize.height
    )
    super.init(contentRect: rect, styleMask: style, backing: backingType, defer: flag)
    self.minSize = MainFlutterWindow.minContentSize
    self.title = "swCutter · TIFF 金字塔切片"
    self.titlebarAppearsTransparent = false
    self.isMovableByWindowBackground = false
  }

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    self.contentViewController = flutterViewController
    // 强制设置窗口大小（覆盖 Nib 里的尺寸）
    self.setContentSize(MainFlutterWindow.defaultContentSize)
    self.center()

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }
}
