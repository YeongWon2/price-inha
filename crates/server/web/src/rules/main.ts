// 관리 화면 · 규칙(server 26차 4번): 메뉴 · 제목 · "준비 중" 빈 상태만 그린다(내용은 뒤 차수).
import { mountMenu } from "../ui/menu";
import { mountAdminFrame } from "../ui/admin";

mountMenu(document.getElementById("menu"), "rules");
void mountAdminFrame(document.getElementById("screen"));
