// 관리 화면 · 규칙(server 28차 2번): 메뉴와 규칙 화면(지금 규칙 · 절대 상한 · 개선안과 승인 · 거절).
import { mountMenu } from "../ui/menu";
import { mountRules } from "./screen";

mountMenu(document.getElementById("menu"), "rules");
void mountRules(document.getElementById("screen"));
