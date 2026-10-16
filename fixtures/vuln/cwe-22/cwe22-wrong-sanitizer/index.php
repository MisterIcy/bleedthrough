<?php

$value = str_replace('../', '', $_GET['x']);
readfile('/var/www/files/' . $value);
