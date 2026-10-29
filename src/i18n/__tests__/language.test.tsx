import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { en } from "../en";
import { t as zh } from "../zh";
import { setLanguage, tr, useI18n, widgetDate } from "..";
import { formatRemaining } from "@/utils/countdown";
import { offsetLabel, REMINDER_PRESETS } from "@/utils/reminders";
import { PRIORITY_LABEL } from "@/types";
import { ReminderPicker } from "@/components/ui/ReminderPicker";
afterEach(()=>act(()=>setLanguage("zh-CN")));
it("英文词典覆盖全部界面词典，并保留插值占位符",()=>{
  function check(obj: object) { for(const value of Object.values(obj)) {
    if(typeof value === "string" && /\p{Script=Han}/u.test(value)) expect(en[value],value).toBeTruthy();
    else if(value && typeof value === "object")check(value);
  }}check(zh);
  for(const [key,value] of Object.entries(en))expect(value.match(/\{\d+\}/g)??[]).toEqual(key.match(/\{\d+\}/g)??[]);
});
it("切换语言即时更新已有组件，保留输入草稿，支持切回中文",()=>{
  function Probe(){const {t}=useI18n();return <><span>{t.settings.title}</span><input aria-label="draft" defaultValue="我的任务"/></>;}
  render(<Probe/>);expect(screen.getByText("设置")).toBeTruthy();
  act(()=>setLanguage("en"));expect(screen.getByText("Settings")).toBeTruthy();
  expect((screen.getByLabelText("draft") as HTMLInputElement).value).toBe("我的任务");
  expect(document.documentElement.lang).toBe("en");
  act(()=>setLanguage("zh-CN"));expect(screen.getByText("设置")).toBeTruthy();
});
it("倒计时、日期、提醒预设和优先级跟随语言",()=>{
  setLanguage("en");expect(formatRemaining(90000)).toBe("1m 30s");
  expect(offsetLabel(1500)).toBe("25 min before");expect(REMINDER_PRESETS[0].label).toBe("1 day before");
  expect(PRIORITY_LABEL.high).toBe("High");expect(widgetDate(Date.now())).not.toMatch(/\p{Script=Han}/u);
  expect(tr("全部稍后 {0} 分钟",5)).toBe("Snooze all for 5 min");
  setLanguage("zh-CN");expect(REMINDER_PRESETS[0].label).toBe("提前 1 天");
});
it("真实提醒表单切换语言后保留未添加的自定义输入",()=>{
  render(<ReminderPicker offsets={[3600]} onChange={()=>{}}/>);
  fireEvent.change(screen.getByLabelText("自定义提前量数值"),{target:{value:"25"}});
  act(()=>setLanguage("en"));
  expect((screen.getByLabelText("Custom reminder amount") as HTMLInputElement).value).toBe("25");
  expect(screen.getByRole("button",{name:"Add"})).toBeTruthy();
  expect(screen.getByText("1 hour before")).toBeTruthy();
  expect(screen.getByRole("option",{name:"min"})).toBeTruthy();
});
