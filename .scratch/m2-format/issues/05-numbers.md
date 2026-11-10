# 05: Numbers

**What to build:** The numbers pass. It converts English number words in the ADR-0015 number style.

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] Cardinals become digits, except one through nine in prose ("three people", "15 people").
- [ ] Money, times, dates and percentages always use digits ("$1,250", "5:30 pm", "March 3", "20%"), including spoken forms like "twelve fifty".
- [ ] Indian grouping when lakh, crore or rupees is spoken ("₹1,25,000", "2 crore"), Western grouping otherwise ("125,000").
- [ ] Hindi number words are left untouched.
- [ ] Golden cases cover ambiguity: "one of them", "a couple", and "two" in "me two" stay as spoken.
- [ ] The latency gate still passes, and formatted WER is re-measured.
