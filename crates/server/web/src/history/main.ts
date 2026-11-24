// 관리 화면 · 히스토리(server 29차 2번): 메뉴와 히스토리 화면(매매 · 하루 정리 · 승인 기록 탭).
import { mountMenu } from "../ui/menu";
import { mountHistory } from "./screen";

mountMenu(document.getElementById("menu"), "history");
void mountHistory(document.getElementById("screen"));
