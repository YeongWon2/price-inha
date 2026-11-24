// 관리 화면 · 템플릿(server 27차 3번): 메뉴와 템플릿 화면(고르기 · 값 맞추기 · 상태 · 승격 기준 · 판 이력 · 멈추기).
import { mountMenu } from "../ui/menu";
import { mountTemplates } from "./screen";

mountMenu(document.getElementById("menu"), "templates");
void mountTemplates(document.getElementById("screen"));
