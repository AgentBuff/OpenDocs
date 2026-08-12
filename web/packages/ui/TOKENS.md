# Open Office Token 与主题指南

UI Token 使用 `--oo-*` 命名空间，并按四层组织：

```text
primitive → semantic → component → product
```

## 四层职责

1. **Primitive**：字号、间距、圆角、阴影、动效、尺寸和 z-index。例如
   `--oo-space-4`、`--oo-radius-md`、`--oo-motion-normal`。
2. **Semantic**：颜色和状态语义，例如 `--oo-color-surface`、`--oo-color-text`、
   `--oo-color-accent`、`--oo-color-danger`。light/dark 只替换这一层的值。
3. **Component**：组件的高度、内边距、焦点环和层级，例如 `--oo-toolbar-height`、
   `--oo-menu-item-height`、`--oo-focus-ring`。
4. **Product**：文档页宽、Block gutter、代码行号宽度等只属于产品组合的参数，应由
   `web/apps/editor` 在自己的作用域提供，不能反向写入基础组件行为。

## 写样式的规则

- 组件样式只能读取语义/组件 Token；不能直接写十六进制颜色、产品阴影或固定主题动效。
- 新 Token 先说明所属层级、用途和 light/dark/density 行为，再加入 `theme.css`。
- 主题作用域由 `.oo-theme-root`、`data-theme` 和 `data-density` 提供；不修改
  `document.body`，不通过全局 class 传播状态。
- `office-light`、`office-dark` 是正式主题名；`compact`、`default`、`comfortable` 是
  独立密度。不要把密度拼进主题名称。
- 动效必须尊重 `prefers-reduced-motion: reduce`。已有控件/浮层会关闭关键动画，新组件也必须
  覆盖自己的 transition/animation。

## 自定义主题

产品可以在自己的作用域覆盖语义/组件 Token：

```css
.my-product-theme {
  --oo-color-accent: #7c3aed;
  --oo-toolbar-height: 42px;
}
```

覆盖只能改变视觉值，不能改变键盘、ARIA、受控状态或 Overlay 生命周期。禁止复制整套
`theme.css`；这样会让新 Token 无法统一演进。

## 验证

主题变更至少需要：light/dark、三种 density、focus-visible、disabled 和 reduced-motion
契约测试；编辑器产品还要执行浏览器回归，确认主题切换不改变 block tree、snapshot 或命令。
