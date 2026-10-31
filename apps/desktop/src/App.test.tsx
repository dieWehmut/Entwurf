import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";

describe("Entwurf workspace", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
  });

  it("renders the project overview with a traceable preview", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: /把教材编译成可追溯的知识图/ })).toBeInTheDocument();
    expect(screen.getByText("内容门禁通过")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "知识图预览" })).toBeInTheDocument();
  });

  it("navigates the five workspace pages", async () => {
    const user = userEvent.setup();
    render(<App />);
    const nav = screen.getByRole("navigation");
    await user.click(within(nav).getByRole("button", { name: /资料与风格/ }));
    expect(screen.getByRole("heading", { name: "先确认来源，再决定构图。" })).toBeInTheDocument();
    await user.click(within(nav).getByRole("button", { name: /^任务/ }));
    expect(screen.getByRole("heading", { name: "任务有版本，自动运行有闸门。" })).toBeInTheDocument();
    await user.click(within(nav).getByRole("button", { name: /^结果/ }));
    expect(screen.getByRole("heading", { name: "内容通过后才允许导出。" })).toBeInTheDocument();
    await user.click(within(nav).getByRole("button", { name: /^项目/ }));
    expect(screen.getByRole("heading", { name: /把教材编译/ })).toBeInTheDocument();
  });

  it("exposes the next theme in both theme controls", async () => {
    const user = userEvent.setup();
    render(<App />);
    const headerTheme = within(screen.getByRole("main")).getByRole("button", { name: /切换到(浅色|深色)主题/ });
    await user.click(headerTheme);
    const expected = document.documentElement.dataset.theme === "dark" ? "切换到浅色主题" : "切换到深色主题";
    expect(within(screen.getByRole("main")).getByRole("button", { name: expected })).toBeInTheDocument();
    expect(within(screen.getByRole("complementary", { name: "主导航" })).getByRole("button", { name: expected })).toBeInTheDocument();
  });

  it("opens the command palette with keyboard shortcut", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.keyboard("{Control>}k{/Control}");
    expect(screen.getByRole("dialog", { name: "命令面板" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps automation off by default and exposes manual handoff", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(within(screen.getByRole("navigation")).getByRole("button", { name: /^任务/ }));
    expect(screen.getByText("自动运行保持关闭")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /复制 TaskEnvelope/ })).toBeInTheDocument();
  });
});
