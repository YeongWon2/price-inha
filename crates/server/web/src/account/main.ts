// 관리 화면 · 계좌(server 30차 2번): 메뉴와 계좌 · 운용 화면(팀 상태 · 다시 시작 · 계좌 · 보유 · 모의계좌 안내).
import { mountMenu } from "../ui/menu";
import { mountAccount } from "./screen";

mountMenu(document.getElementById("menu"), "account");
void mountAccount(document.getElementById("screen"));
